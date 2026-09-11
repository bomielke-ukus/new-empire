//! Commands: the only way anything outside the simulation changes it.
//!
//! A command is issued at one tick and **executed two ticks later**
//! ([`COMMAND_DELAY`]). In single-player that delay is invisible — the
//! presentation layer acknowledges the order immediately — and in lockstep
//! multiplayer it is the window in which every peer's commands for a tick
//! arrive. Building the delay in now is what makes netplay a transport
//! problem later rather than a rewrite.
//!
//! Within a tick, commands execute in `(player, sequence)` order, never in
//! arrival order, so the result does not depend on which peer's packet came
//! first.

use crate::entity::{EntityId, KindId};
use crate::hash::{HashState, StateHasher};
use crate::orders::Rally;
use crate::tech::TechId;
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A player index, `0..MAX_PLAYERS`.
pub type PlayerId = u8;
/// Maximum players in a match.
pub const MAX_PLAYERS: usize = 8;
/// Ticks between issuing a command and executing it.
pub const COMMAND_DELAY: u64 = 2;
/// Largest entity list a single command may name.
///
/// Selection is uncapped for the player (`docs/03` §2), but a command is also
/// something read off disk and, later, off the network, so it needs a bound
/// that corrupt or hostile input cannot exceed. Well above any real selection
/// at the 200 population cap.
pub const MAX_COMMAND_IDS: usize = 8192;

/// What a command asks the simulation to do.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CommandKind {
    /// Create an entity of `kind` at `pos` owned by the issuing player,
    /// free of charge. For tests, scenarios and cheats.
    Spawn {
        /// Static data index.
        kind: KindId,
        /// Tile position.
        pos: Vec2Fx,
    },
    /// Remove an entity. Ignored if it is not the issuer's or is already gone.
    Despawn {
        /// Which entity.
        id: EntityId,
    },
    /// Walk the listed entities to `target`, spreading over nearby tiles.
    Move {
        /// Which entities; stale or foreign handles are skipped.
        ids: Vec<EntityId>,
        /// Destination in tiles.
        target: Vec2Fx,
    },
    /// Cancel whatever the listed entities are doing.
    Stop {
        /// Which entities.
        ids: Vec<EntityId>,
    },
    /// Send villagers to gather from a node.
    Gather {
        /// Villagers.
        ids: Vec<EntityId>,
        /// A tree, bush or vein.
        node: EntityId,
    },
    /// Place a building and send villagers to construct it. The cost is
    /// paid when the site is placed.
    Build {
        /// What to build.
        kind: KindId,
        /// Tile the footprint is centred on.
        x: i32,
        /// Tile the footprint is centred on.
        y: i32,
        /// Villagers to send; may be empty.
        ids: Vec<EntityId>,
    },
    /// Send villagers to help finish a site that already exists.
    Assist {
        /// Villagers.
        ids: Vec<EntityId>,
        /// The site.
        site: EntityId,
    },
    /// Queue a unit at a building. The cost is paid on queueing.
    Train {
        /// The building.
        building: EntityId,
        /// The unit kind.
        kind: KindId,
    },
    /// Remove the last queued item of a kind and refund it.
    CancelTrain {
        /// The building.
        building: EntityId,
    },
    /// Set where a building's produced units go.
    SetRally {
        /// The building.
        building: EntityId,
        /// Destination.
        rally: Rally,
    },
    /// Queue a technology at a building. The cost is paid on queueing.
    Research {
        /// The building.
        building: EntityId,
        /// The technology.
        tech: TechId,
    },
    /// Whether the player's exhausted farms are reseeded automatically.
    SetAutoReseed {
        /// On or off.
        enabled: bool,
    },
}

/// A command with its issuing player.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Command {
    /// The player issuing it. Entities it names must belong to this player.
    pub player: PlayerId,
    /// The request.
    pub kind: CommandKind,
}

/// Why a command was rejected as malformed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandError {
    /// `player` is not a valid player index.
    PlayerOutOfRange {
        /// The offending index.
        player: PlayerId,
    },
    /// The entity list exceeds [`MAX_COMMAND_IDS`].
    TooManyIds {
        /// How many were named.
        len: usize,
    },
}

impl core::fmt::Display for CommandError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CommandError::PlayerOutOfRange { player } => {
                write!(f, "player {player} is not in 0..{MAX_PLAYERS}")
            }
            CommandError::TooManyIds { len } => {
                write!(
                    f,
                    "command names {len} entities, limit is {MAX_COMMAND_IDS}"
                )
            }
        }
    }
}

impl std::error::Error for CommandError {}

impl Command {
    /// Checks that this command is structurally sound.
    ///
    /// Only the structural bound. Semantic authority — "does this player own
    /// that entity?" — is checked by the simulation when the command
    /// executes, because ownership can change between issue and execution.
    pub fn validate(&self) -> Result<(), CommandError> {
        if self.player as usize >= MAX_PLAYERS {
            return Err(CommandError::PlayerOutOfRange {
                player: self.player,
            });
        }
        let named = match &self.kind {
            CommandKind::Move { ids, .. }
            | CommandKind::Stop { ids }
            | CommandKind::Gather { ids, .. }
            | CommandKind::Build { ids, .. }
            | CommandKind::Assist { ids, .. } => ids.len(),
            CommandKind::Spawn { .. }
            | CommandKind::Despawn { .. }
            | CommandKind::Train { .. }
            | CommandKind::CancelTrain { .. }
            | CommandKind::SetRally { .. }
            | CommandKind::Research { .. }
            | CommandKind::SetAutoReseed { .. } => 0,
        };
        if named > MAX_COMMAND_IDS {
            return Err(CommandError::TooManyIds { len: named });
        }
        Ok(())
    }
}

impl HashState for CommandKind {
    /// Discriminant first, then the payload. The discriminant is written even
    /// for variants whose payload would distinguish them anyway, so adding a
    /// variant cannot silently collide with an existing one.
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            CommandKind::Spawn { kind, pos } => {
                h.write_u8(0);
                h.write_u16(*kind);
                h.write(pos);
            }
            CommandKind::Despawn { id } => {
                h.write_u8(1);
                h.write(id);
            }
            CommandKind::Move { ids, target } => {
                h.write_u8(2);
                h.write(ids);
                h.write(target);
            }
            CommandKind::Stop { ids } => {
                h.write_u8(3);
                h.write(ids);
            }
            CommandKind::Gather { ids, node } => {
                h.write_u8(4);
                h.write(ids);
                h.write(node);
            }
            CommandKind::Build { kind, x, y, ids } => {
                h.write_u8(5);
                h.write_u16(*kind);
                h.write_i32(*x);
                h.write_i32(*y);
                h.write(ids);
            }
            CommandKind::Assist { ids, site } => {
                h.write_u8(6);
                h.write(ids);
                h.write(site);
            }
            CommandKind::Train { building, kind } => {
                h.write_u8(7);
                h.write(building);
                h.write_u16(*kind);
            }
            CommandKind::CancelTrain { building } => {
                h.write_u8(8);
                h.write(building);
            }
            CommandKind::SetRally { building, rally } => {
                h.write_u8(9);
                h.write(building);
                h.write(rally);
            }
            CommandKind::Research { building, tech } => {
                h.write_u8(10);
                h.write(building);
                h.write_u16(*tech);
            }
            CommandKind::SetAutoReseed { enabled } => {
                h.write_u8(11);
                h.write_u8(*enabled as u8);
            }
        }
    }
}

impl HashState for Command {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.player);
        h.write(&self.kind);
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct Scheduled {
    seq: u32,
    command: Command,
}

impl Scheduled {
    /// The key commands are ordered by within a tick.
    fn key(&self) -> (PlayerId, u32) {
        (self.command.player, self.seq)
    }
}

impl HashState for Scheduled {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.seq);
        h.write(&self.command);
    }
}

/// Commands waiting for their execution tick.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct CommandQueue {
    pending: BTreeMap<u64, Vec<Scheduled>>,
    next_seq: [u32; MAX_PLAYERS],
}

impl CommandQueue {
    /// An empty queue.
    pub fn new() -> CommandQueue {
        CommandQueue::default()
    }

    /// Schedules `command`, issued at `issue_tick`, for execution
    /// `COMMAND_DELAY` ticks later. Returns the execution tick.
    pub fn schedule(&mut self, issue_tick: u64, command: Command) -> u64 {
        let exec = issue_tick.saturating_add(COMMAND_DELAY);
        self.schedule_at(exec, command);
        exec
    }

    /// Schedules `command` for a specific tick. Used by replay and, later, by
    /// the network layer, which already knows the execution tick.
    ///
    /// The batch for a tick is kept in canonical `(player, seq)` order as
    /// commands arrive, rather than sorted at drain time. That is what makes
    /// two peers who received the same commands in different packet orders
    /// hold *byte-identical* queues, so their state hashes agree before the
    /// commands have even executed.
    pub fn schedule_at(&mut self, exec_tick: u64, command: Command) {
        let p = command.player as usize;
        assert!(p < MAX_PLAYERS, "player id out of range");
        let seq = self.next_seq[p];
        self.next_seq[p] = seq.wrapping_add(1);
        let entry = Scheduled { seq, command };
        let batch = self.pending.entry(exec_tick).or_default();
        let at = batch.partition_point(|s| s.key() < entry.key());
        batch.insert(at, entry);
    }

    /// Removes and returns every command due at or before `tick`, in
    /// canonical `(tick, player, seq)` order.
    pub fn drain_due(&mut self, tick: u64) -> Vec<Command> {
        let mut out = Vec::new();
        // `split_off(&(tick + 1))` overflowed at `u64::MAX` — a panic in
        // debug and, in release, a wrap to 0 that silently drained *nothing*
        // and left the commands pending forever. Draining "everything" is a
        // real call, so the ceiling gets its own branch: a saturating add
        // would instead leave behind the commands scheduled exactly at the
        // ceiling, which is the same bug moved one tick along.
        let due = if tick == u64::MAX {
            std::mem::take(&mut self.pending)
        } else {
            let later = self.pending.split_off(&(tick + 1));
            std::mem::replace(&mut self.pending, later)
        };
        for (_, batch) in due {
            debug_assert!(
                batch.windows(2).all(|w| w[0].key() <= w[1].key()),
                "queue batch left canonical order"
            );
            out.extend(batch.into_iter().map(|s| s.command));
        }
        out
    }

    /// Number of commands not yet executed.
    pub fn pending_len(&self) -> usize {
        self.pending.values().map(Vec::len).sum()
    }
}

impl HashState for CommandQueue {
    /// Hashes the *contents* of the queue, not just its size.
    ///
    /// Two simulations holding different commands for the same future tick
    /// are already divergent, even though nothing observable has happened
    /// yet. Hashing only the count let that divergence hide until the
    /// commands executed, which put the reported desync tick two ticks after
    /// its cause and pointed the investigation at the wrong system.
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.pending.len() as u64);
        for (tick, batch) in &self.pending {
            h.write_u64(*tick);
            h.write(batch);
        }
        for seq in &self.next_seq {
            h.write_u32(*seq);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(player: PlayerId) -> Command {
        Command {
            player,
            kind: CommandKind::Stop { ids: vec![] },
        }
    }

    #[test]
    fn executes_after_delay() {
        let mut q = CommandQueue::new();
        let exec = q.schedule(10, stop(0));
        assert_eq!(exec, 12);
        assert!(q.drain_due(11).is_empty());
        assert_eq!(q.pending_len(), 1);
        assert_eq!(q.drain_due(12).len(), 1);
        assert_eq!(q.pending_len(), 0);
    }

    #[test]
    fn canonical_order_within_a_tick() {
        let mut q = CommandQueue::new();
        // Arrival order: p2, p0, p2, p1. Execution order must be p0, p1, p2, p2.
        q.schedule_at(5, stop(2));
        q.schedule_at(5, stop(0));
        q.schedule_at(5, stop(2));
        q.schedule_at(5, stop(1));
        let players: Vec<_> = q.drain_due(5).into_iter().map(|c| c.player).collect();
        assert_eq!(players, vec![0, 1, 2, 2]);
    }

    #[test]
    fn same_player_keeps_issue_order() {
        let mut q = CommandQueue::new();
        let mk = |k: KindId| Command {
            player: 0,
            kind: CommandKind::Spawn {
                kind: k,
                pos: Vec2Fx::ZERO,
            },
        };
        q.schedule_at(3, mk(7));
        q.schedule_at(3, mk(8));
        q.schedule_at(3, mk(9));
        let kinds: Vec<_> = q
            .drain_due(3)
            .into_iter()
            .map(|c| match c.kind {
                CommandKind::Spawn { kind, .. } => kind,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(kinds, vec![7, 8, 9]);
    }

    /// The lockstep guarantee, in one test: the order commands *arrive* in
    /// must not survive into the queue at all. Two peers whose packets
    /// interleaved differently hold byte-identical queues and therefore agree
    /// on the state hash before the commands have executed.
    #[test]
    fn arrival_order_does_not_reach_the_queue() {
        let mk = |player: PlayerId, kind: KindId| Command {
            player,
            kind: CommandKind::Spawn {
                kind,
                pos: Vec2Fx::ZERO,
            },
        };
        // The same commands per player, four different arrival interleavings.
        let orders: [[(PlayerId, KindId); 6]; 4] = [
            [(0, 1), (0, 2), (1, 1), (1, 2), (2, 1), (2, 2)],
            [(2, 1), (1, 1), (0, 1), (2, 2), (1, 2), (0, 2)],
            [(1, 1), (2, 1), (2, 2), (0, 1), (0, 2), (1, 2)],
            [(0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)],
        ];
        let mut reference: Option<(u64, Vec<(PlayerId, KindId)>)> = None;
        for order in orders {
            let mut q = CommandQueue::new();
            for (player, kind) in order {
                q.schedule_at(5, mk(player, kind));
            }
            let mut h = StateHasher::new();
            h.write(&q);
            let hash = h.finish();
            let drained: Vec<_> = q
                .drain_due(5)
                .into_iter()
                .map(|c| match c.kind {
                    CommandKind::Spawn { kind, .. } => (c.player, kind),
                    _ => unreachable!(),
                })
                .collect();
            match &reference {
                None => reference = Some((hash, drained)),
                Some((h0, d0)) => {
                    assert_eq!(&drained, d0, "execution order depended on arrival order");
                    assert_eq!(hash, *h0, "queue hash depended on arrival order");
                }
            }
        }
        let (_, drained) = reference.unwrap();
        assert_eq!(
            drained,
            vec![(0, 1), (0, 2), (1, 1), (1, 2), (2, 1), (2, 2)]
        );
    }

    // REQ: TA-DET-02
    /// The hash fed only `pending_len()`, so two simulations holding
    /// different pending orders agreed for two ticks and then diverged with
    /// no attributable cause.
    #[test]
    fn queue_hash_covers_contents_not_just_length() {
        let hash = |c: Command, tick: u64| {
            let mut q = CommandQueue::new();
            q.schedule_at(tick, c);
            let mut h = StateHasher::new();
            h.write(&q);
            h.finish()
        };
        let spawn = |k: KindId| Command {
            player: 0,
            kind: CommandKind::Spawn {
                kind: k,
                pos: Vec2Fx::ZERO,
            },
        };
        // Same count, different payload.
        assert_ne!(hash(spawn(1), 5), hash(spawn(2), 5));
        // Same count and payload, different execution tick.
        assert_ne!(hash(spawn(1), 5), hash(spawn(1), 6));
        // Same count, different player.
        assert_ne!(
            hash(stop(0), 5),
            hash(
                Command {
                    player: 1,
                    kind: CommandKind::Stop { ids: vec![] }
                },
                5
            )
        );
        // Every variant must be distinguishable from every other, or a
        // command could be swapped for a different one without the hash
        // noticing.
        let id = EntityId::from_parts(3, 1);
        let variants = [
            CommandKind::Spawn {
                kind: 1,
                pos: Vec2Fx::ZERO,
            },
            CommandKind::Despawn { id },
            CommandKind::Move {
                ids: vec![id],
                target: Vec2Fx::ZERO,
            },
            CommandKind::Stop { ids: vec![id] },
            CommandKind::Gather {
                ids: vec![id],
                node: id,
            },
            CommandKind::Build {
                kind: 1,
                x: 0,
                y: 0,
                ids: vec![id],
            },
            CommandKind::Assist {
                ids: vec![id],
                site: id,
            },
            CommandKind::Train {
                building: id,
                kind: 1,
            },
            CommandKind::CancelTrain { building: id },
            CommandKind::SetRally {
                building: id,
                rally: Rally::None,
            },
        ];
        let mut seen = Vec::new();
        for kind in variants {
            let h = hash(Command { player: 0, kind }, 1);
            assert!(!seen.contains(&h), "two command variants hash the same");
            seen.push(h);
        }
    }

    // REQ: TA-CMD-02
    /// `drain_due(u64::MAX)` is how a caller says "everything". It panicked
    /// in debug and, in release, returned nothing while leaving the commands
    /// queued — the quietest possible way to lose a player's orders.
    #[test]
    fn draining_at_the_maximum_tick_takes_everything() {
        let mut q = CommandQueue::new();
        q.schedule_at(0, stop(0));
        q.schedule_at(u64::MAX, stop(1));
        assert_eq!(q.drain_due(u64::MAX).len(), 2);
        assert_eq!(q.pending_len(), 0);
    }

    /// The delay is added to the issue tick, so a match reaching the end of
    /// the tick counter must not take the process down with it.
    #[test]
    fn scheduling_near_the_tick_ceiling_does_not_overflow() {
        let mut q = CommandQueue::new();
        assert_eq!(q.schedule(u64::MAX - 1, stop(0)), u64::MAX);
        assert_eq!(q.drain_due(u64::MAX).len(), 1);
    }

    #[test]
    fn drains_overdue_ticks_too_in_tick_order() {
        let mut q = CommandQueue::new();
        q.schedule_at(9, stop(1));
        q.schedule_at(7, stop(0));
        q.schedule_at(20, stop(0));
        let players: Vec<_> = q.drain_due(9).into_iter().map(|c| c.player).collect();
        assert_eq!(players, vec![0, 1]);
        assert_eq!(q.pending_len(), 1);
    }
}
