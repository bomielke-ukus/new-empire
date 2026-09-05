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
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A player index, `0..MAX_PLAYERS`.
pub type PlayerId = u8;
/// Maximum players in a match.
pub const MAX_PLAYERS: usize = 8;
/// Ticks between issuing a command and executing it.
pub const COMMAND_DELAY: u64 = 2;

/// What a command asks the simulation to do.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CommandKind {
    /// Create an entity of `kind` at `pos` owned by the issuing player.
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
    /// Walk the listed entities to `target`.
    Move {
        /// Which entities; stale or foreign handles are skipped.
        ids: Vec<EntityId>,
        /// Destination in tiles.
        target: Vec2Fx,
    },
    /// Cancel movement for the listed entities.
    Stop {
        /// Which entities.
        ids: Vec<EntityId>,
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
        let exec = issue_tick + COMMAND_DELAY;
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
        let later = self.pending.split_off(&(tick + 1));
        let due = std::mem::replace(&mut self.pending, later);
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
