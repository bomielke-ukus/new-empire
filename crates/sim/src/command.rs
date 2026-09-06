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
use crate::orders::Rally;
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
            | CommandKind::SetRally { .. } => 0,
        };
        if named > MAX_COMMAND_IDS {
            return Err(CommandError::TooManyIds { len: named });
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct Scheduled {
    seq: u32,
    command: Command,
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
    pub fn schedule_at(&mut self, exec_tick: u64, command: Command) {
        let p = command.player as usize;
        assert!(p < MAX_PLAYERS, "player id out of range");
        let seq = self.next_seq[p];
        self.next_seq[p] = seq.wrapping_add(1);
        self.pending
            .entry(exec_tick)
            .or_default()
            .push(Scheduled { seq, command });
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
        for (_, mut batch) in due {
            batch.sort_by_key(|s| (s.command.player, s.seq));
            out.extend(batch.into_iter().map(|s| s.command));
        }
        out
    }

    /// Number of commands not yet executed.
    pub fn pending_len(&self) -> usize {
        self.pending.values().map(Vec::len).sum()
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
