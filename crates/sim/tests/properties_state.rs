//! Property tests for the command queue and the entity store.
//!
//! Both hold state that, if it ever breaks, produces a desync rather than a
//! visible bug — which is why they get properties rather than examples.

use proptest::prelude::*;
use sim::{
    kinds, Command, CommandKind, CommandQueue, EntityId, Fx, HashState, PlayerId, Rally, Rng,
    StateHasher, Vec2Fx, World, MAX_PLAYERS,
};

fn hash_of<T: HashState>(v: &T) -> u64 {
    let mut h = StateHasher::new();
    h.write(v);
    h.finish()
}

/// Commands across all ten of M2's variants, with plausible and implausible
/// payloads alike — the queue must not care which.
fn any_command() -> impl Strategy<Value = Command> {
    let ids = || {
        prop::collection::vec((any::<u32>(), any::<u32>()), 0..6).prop_map(|v| {
            v.into_iter()
                .map(|(i, g)| EntityId::from_parts(i, g))
                .collect::<Vec<_>>()
        })
    };
    let id = || (any::<u32>(), any::<u32>()).prop_map(|(i, g)| EntityId::from_parts(i, g));
    let kind = prop_oneof![
        (any::<u16>(), -50i32..50, -50i32..50).prop_map(|(k, x, y)| CommandKind::Spawn {
            kind: k,
            pos: Vec2Fx::from_int(x, y)
        }),
        id().prop_map(|id| CommandKind::Despawn { id }),
        (ids(), -50i32..50, -50i32..50).prop_map(|(ids, x, y)| CommandKind::Move {
            ids,
            target: Vec2Fx::from_int(x, y)
        }),
        ids().prop_map(|ids| CommandKind::Stop { ids }),
        (ids(), id()).prop_map(|(ids, node)| CommandKind::Gather { ids, node }),
        (any::<u16>(), -50i32..50, -50i32..50, ids())
            .prop_map(|(kind, x, y, ids)| CommandKind::Build { kind, x, y, ids }),
        (ids(), id()).prop_map(|(ids, site)| CommandKind::Assist { ids, site }),
        (id(), any::<u16>()).prop_map(|(building, kind)| CommandKind::Train { building, kind }),
        id().prop_map(|building| CommandKind::CancelTrain { building }),
        (
            id(),
            prop_oneof![
                Just(Rally::None),
                (-50i32..50, -50i32..50).prop_map(|(x, y)| Rally::Point(Vec2Fx::from_int(x, y))),
                id().prop_map(Rally::Entity),
            ]
        )
            .prop_map(|(building, rally)| CommandKind::SetRally { building, rally }),
    ];
    (0u8..(MAX_PLAYERS as u8), kind).prop_map(|(player, kind)| Command { player, kind })
}

proptest! {
    /// The lockstep guarantee as a property rather than an example: for *any*
    /// set of commands and *any* pair of arrival orders, both peers hold a
    /// byte-identical queue and execute in the same order.
    ///
    /// Without this, two players who both clicked during the same tick would
    /// desync purely on network jitter, and the resulting bug report would be
    /// unreproducible.
    #[test]
    fn queue_state_is_independent_of_arrival_order(
        commands in prop::collection::vec((0u64..4, any_command()), 1..24),
        shuffle in any::<u64>(),
    ) {
        let build = |order: &[(u64, Command)]| {
            let mut q = CommandQueue::new();
            for (tick, c) in order {
                q.schedule_at(*tick, c.clone());
            }
            q
        };

        // Re-interleave the per-player streams at random.
        //
        // A player's own commands must keep their relative order — that is
        // what `seq` records, and a transport that reordered one peer's own
        // packets would be broken at a lower level. What varies between two
        // peers is only *whose* command arrives next, so the permutation this
        // property needs is a random merge of the per-player subsequences,
        // not an arbitrary shuffle.
        let mut streams: Vec<Vec<(u64, Command)>> = vec![Vec::new(); MAX_PLAYERS];
        for entry in &commands {
            streams[entry.1.player as usize].push(entry.clone());
        }
        let mut heads = [0usize; MAX_PLAYERS];
        let mut permuted = Vec::with_capacity(commands.len());
        let mut rng = Rng::new(shuffle);
        while permuted.len() < commands.len() {
            let ready: Vec<usize> = (0..MAX_PLAYERS)
                .filter(|&p| heads[p] < streams[p].len())
                .collect();
            let p = ready[rng.below(ready.len() as u32) as usize];
            permuted.push(streams[p][heads[p]].clone());
            heads[p] += 1;
        }

        let a = build(&commands);
        let b = build(&permuted);
        prop_assert_eq!(hash_of(&a), hash_of(&b), "queue hash depended on arrival order");
        prop_assert_eq!(&a, &b, "queue contents depended on arrival order");

        let mut a = a;
        let mut b = b;
        prop_assert_eq!(a.drain_due(u64::MAX), b.drain_due(u64::MAX));
    }

    #[test]
    fn drain_takes_everything_due_and_nothing_later(
        commands in prop::collection::vec((0u64..20, any_command()), 0..30),
        cutoff in 0u64..20,
    ) {
        let mut q = CommandQueue::new();
        for (tick, c) in &commands {
            q.schedule_at(*tick, c.clone());
        }
        let expected_due = commands.iter().filter(|(t, _)| *t <= cutoff).count();
        let drained = q.drain_due(cutoff);
        prop_assert_eq!(drained.len(), expected_due);
        prop_assert_eq!(q.pending_len(), commands.len() - expected_due);
        // Draining again takes nothing new.
        prop_assert!(q.drain_due(cutoff).is_empty());
    }

    #[test]
    fn drained_commands_are_in_canonical_order(
        commands in prop::collection::vec((0u64..6, any_command()), 1..30),
    ) {
        let mut q = CommandQueue::new();
        let mut ticks = Vec::new();
        for (tick, c) in &commands {
            q.schedule_at(*tick, c.clone());
            ticks.push(*tick);
        }
        // Reconstruct the expected order: by tick, then by player, then by the
        // order the player issued them in.
        let mut expected: Vec<(u64, PlayerId, usize)> = commands
            .iter()
            .enumerate()
            .map(|(i, (t, c))| (*t, c.player, i))
            .collect();
        expected.sort_by_key(|(t, p, i)| (*t, *p, *i));
        let want: Vec<PlayerId> = expected.iter().map(|(_, p, _)| *p).collect();
        let got: Vec<PlayerId> = q.drain_due(u64::MAX).into_iter().map(|c| c.player).collect();
        prop_assert_eq!(got, want);
    }
}

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------

/// One operation against the entity store.
#[derive(Clone, Debug)]
enum Op {
    Spawn {
        kind: u16,
        owner: PlayerId,
        x: i32,
        y: i32,
    },
    /// Despawn the nth *currently live* entity, wrapping.
    DespawnNth(usize),
    /// Despawn using a handle captured earlier, which may now be stale.
    DespawnStale(usize),
}

fn any_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (any::<u16>(), 0u8..(MAX_PLAYERS as u8), -50i32..50, -50i32..50)
            .prop_map(|(kind, owner, x, y)| Op::Spawn { kind, owner, x, y }),
        2 => (0usize..32).prop_map(Op::DespawnNth),
        1 => (0usize..32).prop_map(Op::DespawnStale),
    ]
}

/// Applies `ops` and returns the world plus every handle ever issued.
fn run_ops(ops: &[Op]) -> (World, Vec<EntityId>) {
    let mut w = World::new();
    let mut issued: Vec<EntityId> = Vec::new();
    for op in ops {
        match op {
            Op::Spawn { kind, owner, x, y } => {
                let id = w.spawn(*kind, *owner, Vec2Fx::from_int(*x, *y), Fx::from_int(100));
                issued.push(id);
            }
            Op::DespawnNth(n) => {
                let live: Vec<_> = w.ids().collect();
                if !live.is_empty() {
                    w.despawn(live[n % live.len()]);
                }
            }
            Op::DespawnStale(n) => {
                if !issued.is_empty() {
                    w.despawn(issued[n % issued.len()]);
                }
            }
        }
    }
    (w, issued)
}

proptest! {
    #[test]
    fn invariants_hold_after_any_sequence(ops in prop::collection::vec(any_op(), 0..80)) {
        let (w, _) = run_ops(&ops);
        prop_assert!(w.check().is_ok(), "{:?}", w.check().unwrap_err());
    }

    /// Slot reuse must be lowest-index-first, because the *identity* a new
    /// entity receives is part of the simulation state and two machines that
    /// allocate differently have already diverged.
    #[test]
    fn slot_reuse_is_lowest_index_first(ops in prop::collection::vec(any_op(), 0..80)) {
        let (mut w, _) = run_ops(&ops);
        // The next spawn must land in the lowest free slot, if there is one.
        let free: Vec<usize> = (0..w.capacity())
            .filter(|&i| !w.slots().any(|s| s.index() == i))
            .collect();
        let expected = free.first().copied().unwrap_or(w.capacity());
        let id = w.spawn(1, 0, Vec2Fx::ZERO, Fx::ONE);
        prop_assert_eq!(id.index(), expected);
    }

    /// A handle to a despawned entity must never resolve, even after its slot
    /// has been handed to someone else.
    #[test]
    fn stale_handles_never_resolve(ops in prop::collection::vec(any_op(), 1..80)) {
        let (w, issued) = run_ops(&ops);
        let live: Vec<EntityId> = w.ids().collect();
        for id in issued {
            let resolves = w.slot(id).is_some();
            prop_assert_eq!(resolves, live.contains(&id));
        }
    }

    #[test]
    fn iteration_is_slot_order(ops in prop::collection::vec(any_op(), 0..80)) {
        let (w, _) = run_ops(&ops);
        let indices: Vec<usize> = w.slots().map(|s| s.index()).collect();
        let mut sorted = indices.clone();
        sorted.sort_unstable();
        prop_assert_eq!(indices, sorted);
        prop_assert_eq!(w.slots().count(), w.len());
    }

    /// The despawn-scrub guarantee (`docs/04` §15): what a dead entity *used
    /// to hold* can never influence equality or the hash.
    ///
    /// Two worlds are driven through the same spawn/despawn shape with
    /// completely different component values in the entities that die. They
    /// end with the same slot layout and the same generations, so they must
    /// end identical — otherwise a replay that took a different route to the
    /// same state would report a desync that is not one.
    ///
    /// Note the property deliberately stops short of "same live set implies
    /// equal". Generations and the slot count are part of the state on
    /// purpose: an `EntityId` outstanding in a command queue resolves in one
    /// world and is stale in the other, so those worlds are genuinely
    /// different.
    #[test]
    fn dead_component_values_do_not_survive(
        doomed in prop::collection::vec(
            (any::<u16>(), 0u8..(MAX_PLAYERS as u8), -50i32..50, -50i32..50, any::<i32>()),
            1..12,
        ),
        keep in prop::collection::vec((any::<u16>(), 0u8..(MAX_PLAYERS as u8)), 1..8),
    ) {
        let build = |vary: bool| {
            let mut w = World::new();
            let mut ids = Vec::new();
            for (kind, owner, x, y, res) in &doomed {
                // Every value below differs between the two worlds, and all
                // of these entities are about to die.
                let (k, o, p, r) = if vary {
                    (*kind, *owner, Vec2Fx::from_int(*x, *y), res.abs())
                } else {
                    (kind.wrapping_add(7), owner ^ 1, Vec2Fx::from_int(-*y, *x), res.abs() / 3 + 1)
                };
                let id = w.spawn_with_resource(
                    k,
                    o,
                    p,
                    Fx::from_int(if vary { 33 } else { 91 }),
                    r,
                );
                // Dirty the columns `spawn` does not reach, so all thirteen
                // are covered rather than the five M0 had.
                let i = w.slot(id).unwrap().index();
                w.facing[i] = if vary { 3 } else { 6 };
                w.move_target[i] = Some(if vary { p } else { Vec2Fx::ZERO });
                w.carry[i] = Some((
                    if vary { kinds::Resource::Wood } else { kinds::Resource::Gold },
                    r % 10,
                ));
                w.construction[i] = Some(if vary { 12 } else { 40 });
                w.work[i] = Fx::from_int(if vary { 2 } else { 5 });
                ids.push(id);
            }
            for id in ids {
                w.despawn(id);
            }
            // Same survivors, same values, in both worlds.
            for (i, (kind, owner)) in keep.iter().enumerate() {
                w.spawn(*kind, *owner, Vec2Fx::from_int(i as i32, 0), Fx::from_int(50));
            }
            w
        };
        let a = build(true);
        let b = build(false);
        prop_assert!(a.check().is_ok(), "{:?}", a.check());
        prop_assert_eq!(hash_of(&a), hash_of(&b), "dead component data leaked into the hash");
        prop_assert_eq!(a, b, "dead component data leaked into equality");
    }

    /// The other half of the same guarantee, stated as the thing it protects:
    /// generation and slot count *are* live state, so worlds that differ only
    /// in history must not be conflated.
    #[test]
    fn generations_are_part_of_the_state(kind in any::<u16>()) {
        let mut a = World::new();
        let id = a.spawn(kind, 0, Vec2Fx::ZERO, Fx::ONE);
        a.despawn(id);
        a.spawn(kind, 0, Vec2Fx::ZERO, Fx::ONE);

        let mut b = World::new();
        b.spawn(kind, 0, Vec2Fx::ZERO, Fx::ONE);

        prop_assert_ne!(
            hash_of(&a), hash_of(&b),
            "a recycled slot must not hash the same as a fresh one; the old \
             handle is stale in one world and live in the other"
        );
        prop_assert!(a.slot(id).is_none(), "the old handle must not resolve");
        prop_assert!(b.slot(id).is_some(), "the fresh world's slot 0 is live");
    }

    #[test]
    fn live_count_tracks_reality(ops in prop::collection::vec(any_op(), 0..80)) {
        let (w, _) = run_ops(&ops);
        prop_assert_eq!(w.len(), w.ids().count());
        #[allow(clippy::len_zero)]
        {
            prop_assert_eq!(w.is_empty(), w.len() == 0);
        }
        prop_assert!(w.len() <= w.capacity());
    }
}
