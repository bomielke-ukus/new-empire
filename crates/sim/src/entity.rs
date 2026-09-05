//! The entity store: dense parallel arrays with generational IDs.
//!
//! Hand-rolled rather than an ECS crate because we need two guarantees that
//! general-purpose ECS libraries do not make: **iteration order is slot order,
//! always**, and **freed slots are reused in a deterministic order** (lowest
//! index first). Both are required for two machines to produce the same hash.
//!
//! Systems iterate [`World::slots`] and index the component vectors directly.
//! An [`EntityId`] carries a generation so a stale reference to a despawned
//! entity is detected rather than silently pointing at whoever reused the slot.

use crate::command::PlayerId;
use crate::fx::Fx;
use crate::hash::{HashState, StateHasher};
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};

/// Index into the static unit/building data table. Opaque to this crate.
pub type KindId = u16;

/// A stable handle to an entity. Cheap to copy; survives slot reuse safely.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl EntityId {
    /// The slot this ID refers (or referred) to.
    pub const fn index(self) -> usize {
        self.index as usize
    }
    /// The generation that must match for this ID to be live.
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl HashState for EntityId {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.index);
        h.write_u32(self.generation);
    }
}

/// A validated slot index. Only obtainable from a live lookup, so indexing
/// component vectors with it cannot hit a dead entity in the same tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slot(usize);

impl Slot {
    /// The raw index.
    #[inline]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// All entities and their components.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct World {
    generation: Vec<u32>,
    alive: Vec<bool>,
    /// Free slots, kept sorted descending so `pop()` yields the lowest index.
    free: Vec<u32>,
    live: u32,

    /// What this entity is.
    pub kind: Vec<KindId>,
    /// Who owns it.
    pub owner: Vec<PlayerId>,
    /// Position in tiles.
    pub pos: Vec<Vec2Fx>,
    /// Remaining hit points.
    pub health: Vec<Fx>,
    /// Where it is walking to, if anywhere.
    pub move_target: Vec<Option<Vec2Fx>>,
    /// Resource units remaining, for nodes and carcasses; 0 otherwise.
    pub resource: Vec<i32>,
    /// Which of 8 directions it faces; see [`crate::Angle::facing8`].
    pub facing: Vec<u8>,
}

impl World {
    /// An empty world.
    pub fn new() -> World {
        World::default()
    }

    /// Number of live entities.
    pub fn len(&self) -> usize {
        self.live as usize
    }

    /// True if no entities are live.
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Number of slots ever allocated (live + free).
    pub fn capacity(&self) -> usize {
        self.alive.len()
    }

    /// Creates an entity and returns its handle.
    pub fn spawn(&mut self, kind: KindId, owner: PlayerId, pos: Vec2Fx, health: Fx) -> EntityId {
        self.spawn_with_resource(kind, owner, pos, health, 0)
    }

    /// Creates an entity carrying `resource` units of whatever its kind yields.
    pub fn spawn_with_resource(
        &mut self,
        kind: KindId,
        owner: PlayerId,
        pos: Vec2Fx,
        health: Fx,
        resource: i32,
    ) -> EntityId {
        self.live += 1;
        if let Some(i) = self.free.pop() {
            let i = i as usize;
            self.alive[i] = true;
            self.kind[i] = kind;
            self.owner[i] = owner;
            self.pos[i] = pos;
            self.health[i] = health;
            self.move_target[i] = None;
            self.resource[i] = resource;
            self.facing[i] = 1;
            EntityId {
                index: i as u32,
                generation: self.generation[i],
            }
        } else {
            let i = self.alive.len();
            self.generation.push(0);
            self.alive.push(true);
            self.kind.push(kind);
            self.owner.push(owner);
            self.pos.push(pos);
            self.health.push(health);
            self.move_target.push(None);
            self.resource.push(resource);
            self.facing.push(1);
            EntityId {
                index: i as u32,
                generation: 0,
            }
        }
    }

    /// Removes an entity. Returns false if the handle was already stale.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        let Some(slot) = self.slot(id) else {
            return false;
        };
        let i = slot.0;
        self.alive[i] = false;
        self.generation[i] = self.generation[i].wrapping_add(1);
        // Scrub the slot so a World's derived equality matches its hash:
        // two worlds with the same live state must compare equal whatever
        // used to live in their dead slots.
        self.kind[i] = 0;
        self.owner[i] = 0;
        self.pos[i] = Vec2Fx::ZERO;
        self.health[i] = Fx::ZERO;
        self.move_target[i] = None;
        self.resource[i] = 0;
        self.facing[i] = 0;
        self.live -= 1;
        // Keep `free` sorted descending: insert at the position that
        // maintains order. Slot counts are small enough that the O(n) insert
        // is invisible.
        let idx = id.index;
        let at = self.free.partition_point(|&f| f > idx);
        self.free.insert(at, idx);
        true
    }

    /// True if `id` refers to a live entity.
    pub fn contains(&self, id: EntityId) -> bool {
        self.slot(id).is_some()
    }

    /// Resolves a handle to a slot, or `None` if it is stale or dead.
    pub fn slot(&self, id: EntityId) -> Option<Slot> {
        let i = id.index as usize;
        if i < self.alive.len() && self.alive[i] && self.generation[i] == id.generation {
            Some(Slot(i))
        } else {
            None
        }
    }

    /// The handle for a live slot.
    pub fn id_at(&self, slot: Slot) -> EntityId {
        EntityId {
            index: slot.0 as u32,
            generation: self.generation[slot.0],
        }
    }

    /// Live slots in ascending index order — the only iteration order there is.
    pub fn slots(&self) -> impl Iterator<Item = Slot> + '_ {
        self.alive
            .iter()
            .enumerate()
            .filter(|(_, &a)| a)
            .map(|(i, _)| Slot(i))
    }

    /// Live entity handles in slot order.
    pub fn ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.slots().map(|s| self.id_at(s))
    }
}

impl HashState for World {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.live);
        h.write_u64(self.alive.len() as u64);
        for i in 0..self.alive.len() {
            h.write_bool(self.alive[i]);
            h.write_u32(self.generation[i]);
            if self.alive[i] {
                h.write_u16(self.kind[i]);
                h.write_u8(self.owner[i]);
                h.write(&self.pos[i]);
                h.write(&self.health[i]);
                h.write(&self.move_target[i]);
                h.write_i32(self.resource[i]);
                h.write_u8(self.facing[i]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn(w: &mut World, x: i32) -> EntityId {
        w.spawn(1, 0, Vec2Fx::from_int(x, 0), Fx::from_int(100))
    }

    #[test]
    fn spawn_and_lookup() {
        let mut w = World::new();
        assert!(w.is_empty());
        let a = spawn(&mut w, 1);
        let b = spawn(&mut w, 2);
        assert_eq!(w.len(), 2);
        assert_eq!(a.index(), 0);
        assert_eq!(b.index(), 1);
        assert!(w.contains(a));
        let s = w.slot(b).unwrap();
        assert_eq!(w.pos[s.index()], Vec2Fx::from_int(2, 0));
        assert_eq!(w.id_at(s), b);
    }

    #[test]
    fn despawn_invalidates_handle_and_reuses_slot_with_new_generation() {
        let mut w = World::new();
        let a = spawn(&mut w, 1);
        let _b = spawn(&mut w, 2);
        assert!(w.despawn(a));
        assert!(!w.despawn(a), "double despawn must be rejected");
        assert!(!w.contains(a));
        assert_eq!(w.len(), 1);

        let c = spawn(&mut w, 3);
        assert_eq!(c.index(), 0, "lowest free slot is reused");
        assert_eq!(c.generation(), 1);
        assert!(!w.contains(a), "stale handle still stale after reuse");
        assert!(w.contains(c));
        assert_eq!(w.capacity(), 2);
    }

    #[test]
    fn free_slots_reused_lowest_first_regardless_of_despawn_order() {
        let mut w = World::new();
        let ids: Vec<_> = (0..5).map(|i| spawn(&mut w, i)).collect();
        w.despawn(ids[3]);
        w.despawn(ids[1]);
        w.despawn(ids[4]);
        assert_eq!(spawn(&mut w, 9).index(), 1);
        assert_eq!(spawn(&mut w, 9).index(), 3);
        assert_eq!(spawn(&mut w, 9).index(), 4);
        assert_eq!(spawn(&mut w, 9).index(), 5);
    }

    #[test]
    fn iteration_is_slot_order() {
        let mut w = World::new();
        let ids: Vec<_> = (0..4).map(|i| spawn(&mut w, i)).collect();
        w.despawn(ids[1]);
        let order: Vec<usize> = w.slots().map(|s| s.index()).collect();
        assert_eq!(order, vec![0, 2, 3]);
        let got: Vec<_> = w.ids().collect();
        assert_eq!(got, vec![ids[0], ids[2], ids[3]]);
    }

    #[test]
    fn hash_changes_with_state_and_not_with_history() {
        let mut a = World::new();
        spawn(&mut a, 1);
        spawn(&mut a, 2);

        let mut h1 = StateHasher::new();
        a.hash_state(&mut h1);

        let s = a.slot(a.ids().next().unwrap()).unwrap();
        a.health[s.index()] = Fx::from_int(50);
        let mut h2 = StateHasher::new();
        a.hash_state(&mut h2);
        assert_ne!(h1.finish(), h2.finish());

        // Two worlds built by different paths but with identical state hash equal.
        let mut b = World::new();
        let x = spawn(&mut b, 1);
        spawn(&mut b, 2);
        b.despawn(x);
        let mut c = World::new();
        let y = spawn(&mut c, 7);
        spawn(&mut c, 2);
        c.despawn(y);
        let mut hb = StateHasher::new();
        b.hash_state(&mut hb);
        let mut hc = StateHasher::new();
        c.hash_state(&mut hc);
        assert_eq!(hb.finish(), hc.finish());
        assert_eq!(b, c);
    }
}
