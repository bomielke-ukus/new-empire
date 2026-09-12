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
use crate::kinds::Resource;
use crate::orders::{Formation, Nav, Order, Production, Stance};
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
    /// Builds a handle from its parts.
    ///
    /// For deserialisation, the network layer and tests. Not a way to forge
    /// access: a handle whose generation does not match the slot's is
    /// rejected by [`World::slot`], and the simulation checks ownership
    /// before acting on one. Serde already constructs these from arbitrary
    /// input, so this adds no capability that did not exist.
    pub const fn from_parts(index: u32, generation: u32) -> EntityId {
        EntityId { index, generation }
    }

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
    /// A slot from its index; the caller vouches it is live.
    pub(crate) fn new(i: usize) -> Slot {
        Slot(i)
    }
}

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
    /// Current job.
    pub order: Vec<Order>,
    /// Current trip, for mobile units.
    pub nav: Vec<Option<Nav>>,
    /// What a villager is carrying.
    pub carry: Vec<Option<(Resource, i32)>>,
    /// Ticks of construction work done; `None` once complete (or never a site).
    pub construction: Vec<Option<u32>>,
    /// Production queue and rally point, for buildings that train.
    pub production: Vec<Option<Production>>,
    /// Fractional work accumulator (gathering).
    pub work: Vec<Fx>,
    /// How it answers enemies it was not ordered at.
    #[serde(default)]
    pub stance: Vec<Stance>,
    /// The shape it takes when moved with others.
    #[serde(default)]
    pub formation: Vec<Formation>,
    /// Ticks until it can hit again; 0 means ready.
    #[serde(default)]
    pub reload: Vec<u16>,
    /// Ticks left as a corpse; 0 means alive. A dying entity keeps its slot
    /// so the corpse can be drawn, but takes no part in anything.
    #[serde(default)]
    pub dying: Vec<u16>,
}

/// A structural invariant of the entity store that does not hold.
///
/// Every variant here is a bug that would otherwise surface much later as an
/// unexplained hash divergence. Naming the broken invariant, and the slot,
/// turns "the hash diverged at tick 9,900" into "the free list gained a
/// duplicate at tick 4,132".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorldViolation {
    /// The parallel component vectors are different lengths.
    RaggedColumns {
        /// Which column.
        column: &'static str,
        /// Its length.
        len: usize,
        /// The length every column should have.
        expected: usize,
    },
    /// The cached live count disagrees with the `alive` flags.
    LiveCountWrong {
        /// The cached count.
        cached: u32,
        /// The counted one.
        counted: u32,
    },
    /// The free list is not sorted descending, so slot reuse would stop being
    /// lowest-index-first and two machines would allocate differently.
    FreeListUnsorted {
        /// Position of the first out-of-order pair.
        at: usize,
    },
    /// The free list names the same slot twice.
    FreeListDuplicate {
        /// The repeated slot.
        slot: u32,
    },
    /// The free list names a slot that is live.
    FreeSlotIsLive {
        /// The slot.
        slot: u32,
    },
    /// The free list names a slot that does not exist.
    FreeSlotOutOfRange {
        /// The slot.
        slot: u32,
    },
    /// A dead slot still holds component data, which breaks the guarantee
    /// that a world's identity is its live state and not its history.
    DeadSlotNotScrubbed {
        /// The slot.
        slot: u32,
        /// Which column still holds something.
        column: &'static str,
    },
    /// A slot is neither live nor free, so it can never be used again.
    SlotLeaked {
        /// The slot.
        slot: u32,
    },
}

impl core::fmt::Display for WorldViolation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WorldViolation::RaggedColumns {
                column,
                len,
                expected,
            } => write!(f, "column `{column}` has {len} rows, expected {expected}"),
            WorldViolation::LiveCountWrong { cached, counted } => {
                write!(f, "live count is {cached} but {counted} slots are alive")
            }
            WorldViolation::FreeListUnsorted { at } => {
                write!(f, "free list is not sorted descending at index {at}")
            }
            WorldViolation::FreeListDuplicate { slot } => {
                write!(f, "free list names slot {slot} twice")
            }
            WorldViolation::FreeSlotIsLive { slot } => {
                write!(f, "slot {slot} is both live and free")
            }
            WorldViolation::FreeSlotOutOfRange { slot } => {
                write!(f, "free list names slot {slot}, which does not exist")
            }
            WorldViolation::DeadSlotNotScrubbed { slot, column } => {
                write!(f, "dead slot {slot} still holds `{column}` data")
            }
            WorldViolation::SlotLeaked { slot } => {
                write!(f, "slot {slot} is neither live nor free")
            }
        }
    }
}

impl std::error::Error for WorldViolation {}

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
            self.order[i] = Order::Idle;
            self.nav[i] = None;
            self.carry[i] = None;
            self.construction[i] = None;
            self.production[i] = None;
            self.work[i] = Fx::ZERO;
            self.stance[i] = Stance::default_for(kind);
            self.formation[i] = Formation::default_for(kind);
            self.reload[i] = 0;
            self.dying[i] = 0;
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
            self.order.push(Order::Idle);
            self.nav.push(None);
            self.carry.push(None);
            self.construction.push(None);
            self.production.push(None);
            self.work.push(Fx::ZERO);
            self.stance.push(Stance::default_for(kind));
            self.formation.push(Formation::default_for(kind));
            self.reload.push(0);
            self.dying.push(0);
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
        self.order[i] = Order::Idle;
        self.nav[i] = None;
        self.carry[i] = None;
        self.construction[i] = None;
        self.production[i] = None;
        self.work[i] = Fx::ZERO;
        self.stance[i] = Stance::default();
        self.formation[i] = Formation::default();
        self.reload[i] = 0;
        self.dying[i] = 0;
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

    /// Verifies every structural invariant of the store.
    ///
    /// Not on the hot path. Tests, the corpus runner and the soak runner call
    /// it after each tick, and `--features debug-checks` makes
    /// [`crate::Simulation::step`] call it too. The point is to fail at the
    /// tick that *created* an inconsistency rather than at the much later
    /// tick that noticed.
    pub fn check(&self) -> Result<(), WorldViolation> {
        let n = self.alive.len();
        let columns: [(&'static str, usize); 17] = [
            ("stance", self.stance.len()),
            ("formation", self.formation.len()),
            ("reload", self.reload.len()),
            ("dying", self.dying.len()),
            ("generation", self.generation.len()),
            ("kind", self.kind.len()),
            ("owner", self.owner.len()),
            ("pos", self.pos.len()),
            ("health", self.health.len()),
            ("move_target", self.move_target.len()),
            ("resource", self.resource.len()),
            ("facing", self.facing.len()),
            ("order", self.order.len()),
            ("nav", self.nav.len()),
            ("carry", self.carry.len()),
            ("construction", self.construction.len()),
            ("production", self.production.len()),
        ];
        for (column, len) in columns {
            if len != n {
                return Err(WorldViolation::RaggedColumns {
                    column,
                    len,
                    expected: n,
                });
            }
        }
        if self.work.len() != n {
            return Err(WorldViolation::RaggedColumns {
                column: "work",
                len: self.work.len(),
                expected: n,
            });
        }

        let counted = self.alive.iter().filter(|&&a| a).count() as u32;
        if counted != self.live {
            return Err(WorldViolation::LiveCountWrong {
                cached: self.live,
                counted,
            });
        }

        let mut is_free = vec![false; n];
        for (at, pair) in self.free.windows(2).enumerate() {
            if pair[0] <= pair[1] {
                return Err(WorldViolation::FreeListUnsorted { at });
            }
        }
        for &slot in &self.free {
            let i = slot as usize;
            if i >= n {
                return Err(WorldViolation::FreeSlotOutOfRange { slot });
            }
            if is_free[i] {
                return Err(WorldViolation::FreeListDuplicate { slot });
            }
            if self.alive[i] {
                return Err(WorldViolation::FreeSlotIsLive { slot });
            }
            is_free[i] = true;
        }

        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            if self.alive[i] {
                continue;
            }
            if !is_free[i] {
                return Err(WorldViolation::SlotLeaked { slot: i as u32 });
            }
            // Every column `despawn` scrubs is checked, so adding a component
            // without scrubbing it shows up here rather than as a hash that
            // depends on history.
            let scrubbed: [(&'static str, bool); 17] = [
                ("stance", self.stance[i] == Stance::default()),
                ("formation", self.formation[i] == Formation::default()),
                ("reload", self.reload[i] == 0),
                ("dying", self.dying[i] == 0),
                ("kind", self.kind[i] == 0),
                ("owner", self.owner[i] == 0),
                ("pos", self.pos[i] == Vec2Fx::ZERO),
                ("health", self.health[i] == Fx::ZERO),
                ("move_target", self.move_target[i].is_none()),
                ("resource", self.resource[i] == 0),
                ("facing", self.facing[i] == 0),
                ("order", self.order[i] == Order::Idle),
                ("nav", self.nav[i].is_none()),
                ("carry", self.carry[i].is_none()),
                ("construction", self.construction[i].is_none()),
                ("production", self.production[i].is_none()),
                ("work", self.work[i] == Fx::ZERO),
            ];
            for (column, ok) in scrubbed {
                if !ok {
                    return Err(WorldViolation::DeadSlotNotScrubbed {
                        slot: i as u32,
                        column,
                    });
                }
            }
        }
        Ok(())
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
                h.write(&self.order[i]);
                h.write(&self.nav[i]);
                match self.carry[i] {
                    None => h.write_u8(0),
                    Some((r, n)) => {
                        h.write_u8(1 + r.index() as u8);
                        h.write_i32(n);
                    }
                }
                h.write(&self.construction[i]);
                h.write(&self.production[i]);
                h.write(&self.work[i]);
                h.write_u8(self.stance[i] as u8);
                h.write_u8(self.formation[i] as u8);
                h.write_u16(self.reload[i]);
                h.write_u16(self.dying[i]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scrub contract, column by column.
    ///
    /// `despawn` has to clear every component, or a world's identity starts
    /// depending on its history and two replays that reach the same live
    /// state report a desync that is not one. M2's own suite never despawns
    /// a resource-bearing entity — an exhausted node is already zero — so
    /// without this the check has no coverage at all.
    #[test]
    fn despawn_scrubs_every_column() {
        let mut w = World::new();
        let id = w.spawn(7, 1, Vec2Fx::from_int(3, 4), Fx::from_int(50));
        let i = w.slot(id).unwrap().index();
        // Dirty every column the scrub is responsible for.
        w.resource[i] = 250;
        w.facing[i] = 5;
        w.move_target[i] = Some(Vec2Fx::from_int(9, 9));
        w.carry[i] = Some((crate::kinds::Resource::Wood, 7));
        w.construction[i] = Some(40);
        w.work[i] = Fx::from_int(3);
        assert!(w.check().is_ok(), "a live entity may hold anything");

        w.despawn(id);
        w.check().expect("every column must be scrubbed on despawn");

        // And a world that never saw that entity is indistinguishable.
        let mut fresh = World::new();
        let fid = fresh.spawn(7, 1, Vec2Fx::from_int(3, 4), Fx::from_int(50));
        fresh.despawn(fid);
        assert_eq!(w, fresh, "history leaked past the scrub");
    }

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
