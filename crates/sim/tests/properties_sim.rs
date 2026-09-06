//! Property tests for the simulation as a whole, across the config space.
//!
//! The corpus pins ten hand-chosen matches. These sweep the config space
//! instead: random map kinds, sizes, player counts, entity caps and
//! population limits, each driven with a randomised command stream. The
//! configs a bug hides in are the ones nobody thought to write a scenario for.

use proptest::prelude::*;
use sim::{
    kinds, Command, CommandKind, EntityId, MapKind, MapSpec, PlayerId, Replay, Rng, SimConfig,
    Simulation, Vec2Fx, MAX_PLAYERS,
};

/// Configs across the ranges the game ships, plus the clamped extremes.
fn any_config() -> impl Strategy<Value = SimConfig> {
    (
        prop_oneof![2 => Just(MapKind::Flat), 8 => Just(MapKind::Inland)],
        // `MapSpec` clamps to 48..=256; hit both ends and the middle.
        prop_oneof![6 => 48u16..200, 1 => Just(48u16), 1 => Just(256u16)],
        1u8..(MAX_PLAYERS as u8 + 1),
        prop_oneof![6 => 200u32..4000, 1 => Just(0u32), 1 => Just(1u32)],
        prop_oneof![6 => 1u32..200, 1 => Just(0u32)],
        any::<bool>(),
    )
        .prop_map(
            |(kind, size, players, max_entities, pop_cap_max, wander)| SimConfig {
                map: MapSpec {
                    kind,
                    size,
                    players,
                },
                max_entities,
                wander,
                pop_cap_max,
            },
        )
}

/// Drives a match with a reproducible pseudo-random command stream, calling
/// `after_tick` after every tick.
fn drive(
    seed: u64,
    config: &SimConfig,
    ticks: u64,
    mut after_tick: impl FnMut(&Simulation),
) -> Simulation {
    let mut sim = Simulation::new(seed, config.clone());
    let mut bot = Rng::new(seed ^ 0xD1CE);
    let players = config.map.players.clamp(1, 8);
    let map = sim.map().width();

    while sim.tick() < ticks {
        for player in 0..players {
            if !bot.chance(1, 5) {
                continue;
            }
            let mine: Vec<EntityId> = sim
                .world()
                .slots()
                .filter(|s| sim.world().owner[s.index()] == player)
                .map(|s| sim.world().id_at(s))
                .collect();
            let any_node: Vec<EntityId> = sim
                .world()
                .slots()
                .filter(|s| sim.world().resource[s.index()] > 0)
                .map(|s| sim.world().id_at(s))
                .take(8)
                .collect();
            // Deliberately off-map sometimes: the sim must clamp, not trust.
            let target =
                Vec2Fx::from_int(bot.range_i32(-12, map + 12), bot.range_i32(-12, map + 12));
            let kind = match bot.below(8) {
                0 => CommandKind::Spawn {
                    kind: kinds::VILLAGER,
                    pos: target,
                },
                1 if !mine.is_empty() => CommandKind::Despawn {
                    id: mine[bot.below(mine.len() as u32) as usize],
                },
                2 if !mine.is_empty() && !any_node.is_empty() => CommandKind::Gather {
                    ids: mine.clone(),
                    node: any_node[bot.below(any_node.len() as u32) as usize],
                },
                3 => CommandKind::Build {
                    kind: kinds::HOUSE,
                    x: bot.range_i32(-4, map + 4),
                    y: bot.range_i32(-4, map + 4),
                    ids: mine.clone(),
                },
                4 if !mine.is_empty() => CommandKind::Train {
                    building: mine[0],
                    kind: kinds::VILLAGER,
                },
                5 => CommandKind::Stop { ids: mine.clone() },
                _ => CommandKind::Move {
                    ids: mine.clone(),
                    target,
                },
            };
            let command = Command { player, kind };
            // Everything this generates must be structurally sound, or the
            // sweep is testing the validator rather than the simulation.
            assert!(command.validate().is_ok(), "{command:?}");
            sim.issue(command);
        }
        sim.step();
        after_tick(&sim);
    }
    sim
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Determinism is not a property of one config.
    #[test]
    fn any_config_replays_identically(
        seed in any::<u64>(),
        config in any_config(),
        ticks in 20u64..150,
    ) {
        let trace = |s: u64| {
            let mut hashes = Vec::new();
            drive(s, &config, ticks, |sim| hashes.push(sim.state_hash()));
            hashes
        };
        prop_assert_eq!(trace(seed), trace(seed));
    }

    /// Every invariant, after every tick, across the config space. This is
    /// what turns a corrupted world into a named failure at the tick that
    /// caused it rather than a hash divergence a thousand ticks later.
    #[test]
    fn invariants_hold_at_every_tick(
        seed in any::<u64>(),
        config in any_config(),
        ticks in 20u64..120,
    ) {
        let mut failure = None;
        drive(seed, &config, ticks, |sim| {
            if failure.is_none() {
                if let Err(v) = sim.check() {
                    failure = Some((sim.tick(), v));
                }
            }
        });
        prop_assert!(failure.is_none(), "{:?}", failure);
    }

    /// Anything the simulation records must replay to the same state.
    #[test]
    fn recorded_matches_replay_to_the_same_state(
        seed in any::<u64>(),
        config in any_config(),
        ticks in 20u64..120,
    ) {
        let live = drive(seed, &config, ticks, |_| {});
        let live_hash = live.state_hash();
        let replay = live.replay();
        prop_assert!(replay.validate().is_ok(), "{:?}", replay.validate());
        let replayed = replay.run(|_, _| {}).unwrap();
        prop_assert_eq!(replayed.state_hash(), live_hash);
        prop_assert_eq!(replayed, live);
        prop_assert_eq!(replay.verify().unwrap(), live_hash);
        prop_assert_eq!(replay.trace_digest().unwrap(), replay.trace_digest().unwrap());
    }

    /// A replay must survive the round trip through its on-disk form.
    #[test]
    fn replays_round_trip_through_ron(
        seed in any::<u64>(),
        ticks in 20u64..80,
    ) {
        let config = SimConfig::default();
        let live = drive(seed, &config, ticks, |_| {});
        let replay = live.replay();
        let text = ron::to_string(&replay).unwrap();
        let back: Replay = ron::from_str(&text).unwrap();
        prop_assert_eq!(&back, &replay);
        prop_assert_eq!(back.trace_digest().unwrap(), replay.trace_digest().unwrap());
    }

    /// A player may only ever steer their own units (`docs/04` §2 rule 5).
    ///
    /// Stated over *orders*, not positions. A neutral gazelle standing where
    /// player 0's villagers want to walk gets displaced by the separation
    /// pass, and that is correct physics rather than a stolen unit — the
    /// first version of this property asserted on position and failed on
    /// exactly that. What must never happen is another player's unit
    /// *accepting* the order.
    #[test]
    fn a_player_cannot_give_orders_to_another_players_units(
        seed in any::<u64>(),
        ticks in 5u64..40,
    ) {
        let config = SimConfig {
            wander: false,
            ..SimConfig::default()
        };
        let mut sim = Simulation::new(seed, config);
        let before: Vec<(EntityId, PlayerId, String)> = sim
            .world()
            .slots()
            .filter(|s| kinds::info(sim.world().kind[s.index()]).mobile)
            .map(|s| {
                (
                    sim.world().id_at(s),
                    sim.world().owner[s.index()],
                    format!("{:?}", sim.world().order[s.index()]),
                )
            })
            .collect();
        prop_assume!(before.iter().any(|(_, o, _)| *o == 0));
        prop_assume!(before.iter().any(|(_, o, _)| *o != 0));

        // Player 0 orders *everyone*, including units it does not own.
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: before.iter().map(|(id, _, _)| *id).collect(),
                target: Vec2Fx::from_int(1, 1),
            },
        });
        for _ in 0..ticks {
            sim.step();
        }

        let mut moved_own = false;
        for (id, owner, was) in before {
            let Some(slot) = sim.world().slot(id) else {
                continue;
            };
            let now = format!("{:?}", sim.world().order[slot.index()]);
            if owner == 0 {
                moved_own |= now != was;
            } else {
                prop_assert_eq!(
                    &now,
                    &was,
                    "player {} took an order from player 0",
                    owner
                );
                prop_assert_eq!(
                    sim.world().owner[slot.index()],
                    owner,
                    "ownership changed under {:?}",
                    id
                );
            }
        }
        prop_assert!(moved_own, "player 0's own units ignored the order");
    }
}
