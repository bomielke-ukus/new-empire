//! The pathfinding behaviour requirements from `docs/04` §5, which introduces
//! them as "Behaviour requirements, tested explicitly".
//!
//! [TA-PATH-02] joined the file with M4's flow fields: a blocked walker no
//! longer waits out a forty-tick stall and a replan allowance, it asks the
//! field for a new heading within three ticks, and the field is rebuilt from
//! the tiles that changed.

mod common;
use common::{index_of, inland, move_to, owned, pos_of, run, spawn};
use sim::{kinds, Fx, NavState, Order, SimConfig, Simulation, Vec2Fx};

/// Somewhere open, far enough away that the route is shared for a while.
fn far_target(sim: &Simulation, from: Vec2Fx) -> Vec2Fx {
    let w = sim.nav().width();
    let h = sim.nav().height();
    // Aim for the opposite quadrant, then snap to the nearest passable tile so
    // the order cannot fail for a reason unrelated to what is being tested.
    let t = sim::nav::tile_of(from);
    let (tx, ty) = (
        if t.0 * 2 < w { w - 6 } else { 5 },
        if t.1 * 2 < h { h - 6 } else { 5 },
    );
    let tile = sim
        .nav()
        .nearest_passable(tx, ty, 20, Some(t))
        .expect("somewhere passable in the far quadrant");
    sim::nav::centre(tile)
}

// ---------------------------------------------------------------------------
// TA-PATH-02 — repath when blocked
// ---------------------------------------------------------------------------

/// A villager walking a corridor has a wall of houses dropped across it two
/// tiles ahead. Within three ticks it must be heading somewhere else, and it
/// must still get there.
///
/// REQ: TA-PATH-02
#[test]
fn a_walker_whose_path_is_blocked_repaths_within_three_ticks() {
    let mut sim = Simulation::new(
        11,
        SimConfig {
            map: sim::MapSpec {
                kind: sim::MapKind::Flat,
                size: 48,
                players: 1,
            },
            wander: false,
            ..SimConfig::default()
        },
    );
    // A corridor: walls north and south of row 20, open at x = 10..40.
    let house = kinds::info(kinds::HOUSE).footprint as i32;
    let mut walls = Vec::new();
    for x in (8..44).step_by(2) {
        for y in [16, 24] {
            sim.issue(spawn(
                0,
                kinds::HOUSE,
                sim::nav::building_centre(x, y, house),
            ));
        }
    }
    sim.issue(spawn(0, kinds::VILLAGER, sim::nav::centre((10, 20))));
    run(&mut sim, 3);
    let v = owned(&sim, 0, kinds::VILLAGER)[0];
    let target = sim::nav::centre((40, 20));
    sim.issue(move_to(0, vec![v], target));
    run(&mut sim, sim::COMMAND_DELAY as u32 + 2);
    let heading_before = sim.world().nav[index_of(&sim, v)]
        .as_ref()
        .and_then(|n| n.waypoints.first().copied())
        .expect("walking");
    assert!(
        heading_before.x > Fx::from_int(12),
        "walking east along the corridor: {heading_before:?}"
    );

    // Drop a wall across the corridor two tiles ahead of the villager.
    let vx = pos_of(&sim, v).x.floor();
    for y in 17..=23 {
        walls.push((vx + 3, y));
        sim.issue(spawn(
            0,
            kinds::HOUSE,
            sim::nav::building_centre(vx + 3, y, house),
        ));
    }
    // The wall lands after the command delay; from that tick, count.
    run(&mut sim, sim::COMMAND_DELAY as u32 + 1);
    assert!(
        walls.iter().all(|&(x, y)| !sim.nav().passable(x, y)),
        "the wall must actually block the corridor"
    );
    let mut repathed_after = None;
    for tick in 1..=3 {
        sim.step();
        let i = index_of(&sim, v);
        let heading = sim.world().nav[i]
            .as_ref()
            .and_then(|n| n.waypoints.first().copied());
        if heading.is_some_and(|h| h != heading_before) {
            repathed_after = Some(tick);
            break;
        }
    }
    assert!(
        repathed_after.is_some(),
        "no new heading within three ticks of the wall going up"
    );
    // The corridor is sealed east of the wall, so the route now goes round
    // the outside of the houses. It must still arrive.
    run(&mut sim, 20 * 150);
    let i = index_of(&sim, v);
    assert_eq!(
        sim.world().order[i],
        Order::Idle,
        "still walking after 150 s"
    );
    assert!(
        pos_of(&sim, v).distance(target) < Fx::from_int(3),
        "ended at {:?}, not by the target",
        pos_of(&sim, v)
    );
}

// ---------------------------------------------------------------------------
// TA-PATH-04 — overtaking
// ---------------------------------------------------------------------------

/// A Scout (1.6 tiles/s) and a Villager (0.9) sent along the same route from
/// the same place. Arriving first is not enough: a unit that was never behind
/// has not overtaken anything, so this also checks the order actually swaps
/// while both are still walking.
///
/// REQ: TA-PATH-04
#[test]
fn a_faster_unit_overtakes_a_slower_one_on_a_shared_route() {
    let mut sim = inland(7);
    let villager = owned(&sim, 0, kinds::VILLAGER)[0];
    let start = pos_of(&sim, villager);

    // Put the scout a little *behind* the villager along the route, so it has
    // something to overtake rather than merely a head start.
    let target = far_target(&sim, start);
    let back = start - (target - start).scale_ratio(Fx::from_ratio(1, 20), Fx::ONE);
    let back_tile = sim
        .nav()
        .nearest_passable(
            sim::nav::tile_of(back).0,
            sim::nav::tile_of(back).1,
            10,
            Some(sim::nav::tile_of(start)),
        )
        .expect("a passable tile behind the villager");
    sim.issue(spawn(0, kinds::SCOUT, sim::nav::centre(back_tile)));
    run(&mut sim, 5);
    let scout = *owned(&sim, 0, kinds::SCOUT)
        .first()
        .expect("the scout spawned");

    assert!(
        pos_of(&sim, scout).distance(target) > pos_of(&sim, villager).distance(target),
        "the fixture is wrong: the scout must start further from the target \
         than the villager, or there is nothing to overtake"
    );

    sim.issue(move_to(0, vec![villager, scout], target));

    // Sample who is closer to the target each tick while both still walk.
    let mut scout_was_behind = false;
    let mut scout_got_ahead_while_both_walking = false;
    for _ in 0..20 * 240 {
        sim.step();
        let (vi, si) = (index_of(&sim, villager), index_of(&sim, scout));
        let both_walking = [vi, si]
            .iter()
            .all(|&i| matches!(&sim.world().nav[i], Some(n) if n.state == NavState::Walking));
        let scout_ahead =
            sim.world().pos[si].distance(target) < sim.world().pos[vi].distance(target);
        if both_walking && !scout_ahead {
            scout_was_behind = true;
        }
        if both_walking && scout_ahead && scout_was_behind {
            scout_got_ahead_while_both_walking = true;
            break;
        }
        if !both_walking && scout_got_ahead_while_both_walking {
            break;
        }
    }
    assert!(
        scout_was_behind,
        "the scout was never behind, so the test never observed an overtake"
    );
    assert!(
        scout_got_ahead_while_both_walking,
        "the scout never passed the villager while both were walking — \
         [TA-PATH-04] is the 1997 frustration this project set out to fix"
    );
}

// ---------------------------------------------------------------------------
// TA-PATH-05 — overlap
// ---------------------------------------------------------------------------

/// Units pushed together must part. The spec says overlap resolves "by entity
/// ID order"; what the code does is visit each pair once in **slot index**
/// order (`if ju > i`), which for live entities is the same ordering because
/// the index dominates the id. Exactly-coincident units are a separate branch
/// that parts them along an axis chosen by slot parity.
///
/// REQ: TA-PATH-05
#[test]
fn a_crowd_spread_over_one_tile_parts_instead_of_stacking() {
    let mut sim = inland(3);
    let (sx, sy) = sim.starts()[0];
    let spot = sim::nav::centre((sx + 5, sy + 5));
    for _ in 0..8 {
        sim.issue(spawn(0, kinds::VILLAGER, spot));
    }
    run(&mut sim, 60);

    let ids = owned(&sim, 0, kinds::VILLAGER);
    let positions: Vec<Vec2Fx> = ids.iter().map(|&id| pos_of(&sim, id)).collect();
    for a in 0..positions.len() {
        for b in (a + 1)..positions.len() {
            assert_ne!(
                positions[a], positions[b],
                "two units are at exactly the same point: {a} and {b}"
            );
        }
    }
}

/// The `d.is_zero()` branch of the separation pass: two units at *precisely*
/// the same point have no direction to push apart along, so one is chosen.
/// Nothing else in the suite reaches this, and a unit pair that stays welded
/// together is a stuck unit.
///
/// REQ: TA-PATH-05
#[test]
fn two_exactly_coincident_units_separate() {
    let mut sim = inland(3);
    let (sx, sy) = sim.starts()[0];
    let spot = sim::nav::centre((sx + 6, sy + 6));
    sim.issue(spawn(0, kinds::VILLAGER, spot));
    sim.issue(spawn(0, kinds::VILLAGER, spot));
    run(&mut sim, 3);

    let ids = owned(&sim, 0, kinds::VILLAGER);
    let pair: Vec<_> = ids
        .iter()
        .copied()
        .filter(|&id| pos_of(&sim, id).distance(spot) < Fx::ONE)
        .collect();
    assert!(
        pair.len() >= 2,
        "the fixture is wrong: two villagers should have spawned on {spot:?}"
    );
    run(&mut sim, 40);
    let (a, b) = (pos_of(&sim, pair[0]), pos_of(&sim, pair[1]));
    assert_ne!(a, b, "coincident units stayed welded together");
}

/// Resolution must be a function of the state, not of anything outside it.
/// Two runs from the same seed must settle a crowd identically — otherwise
/// "deterministically" in [TA-PATH-05] is not true and every replay is void.
///
/// REQ: TA-PATH-05
#[test]
fn the_same_crowd_settles_identically_twice() {
    let settle = |seed: u64| {
        let mut sim = inland(seed);
        let (sx, sy) = sim.starts()[0];
        for k in 0..10 {
            sim.issue(spawn(
                0,
                kinds::VILLAGER,
                sim::nav::centre((sx + 4 + k % 3, sy + 4 + k / 3)),
            ));
        }
        run(&mut sim, 120);
        (
            sim.state_hash(),
            owned(&sim, 0, kinds::VILLAGER)
                .iter()
                .map(|&id| pos_of(&sim, id))
                .collect::<Vec<_>>(),
        )
    };
    let (h1, p1) = settle(21);
    let (h2, p2) = settle(21);
    assert_eq!(h1, h2, "the same seed must produce the same state hash");
    assert_eq!(p1, p2, "and the same resolved positions");
}

// ---------------------------------------------------------------------------
// TA-PATH-06 — the path budget
// ---------------------------------------------------------------------------

/// [TA-PATH-06] has two halves. The budget half: a tick serves a fixed
/// number of *destinations* (`DESTINATIONS_PER_TICK`), and a unit bound
/// for one beyond that waits, counted in `TickStats::path_deferred`. The
/// requirement's actual content is that a deferred request **waits a tick**
/// rather than being dropped, so this test orders more distinct trips than
/// one tick serves and then insists every unit is eventually served.
///
/// Destinations, not units: forty units sent to one place share one field
/// and cost one budget slot, which is the point of flow fields.
///
/// The **priority** half — "player-issued orders before AI-issued ones" — is
/// not implemented and cannot be observed yet: destinations are served in
/// slot order, and there is no `ai` crate to issue a competing order. It
/// stays owed to M5.
///
/// REQ: TA-PATH-06
#[test]
fn over_budget_path_requests_wait_a_tick_rather_than_being_dropped() {
    let mut sim = Simulation::new(
        9,
        SimConfig {
            map: sim::MapSpec {
                kind: sim::MapKind::Inland,
                size: 128,
                players: 2,
            },
            max_entities: 4000,
            wander: false,
            ..SimConfig::default()
        },
    );
    let (sx, sy) = sim.starts()[0];
    for k in 0..40 {
        sim.issue(spawn(
            0,
            kinds::VILLAGER,
            sim::nav::centre((sx + 3 + k % 8, sy + 3 + k / 8)),
        ));
    }
    run(&mut sim, 10);
    let ids = owned(&sim, 0, kinds::VILLAGER);
    assert!(ids.len() >= 40, "expected a crowd, got {}", ids.len());

    // Forty trips to forty different far-off tiles, all on the same tick, so
    // forty destinations compete for a tick that serves sixteen.
    let start = pos_of(&sim, ids[0]);
    let far = far_target(&sim, start);
    let (fx, fy) = sim::nav::tile_of(far);
    for (k, &id) in ids.iter().enumerate().take(40) {
        let (dx, dy) = ((k % 8) as i32 * 2, (k / 8) as i32 * 2);
        let tile = sim
            .nav()
            .nearest_passable(fx - 8 + dx, fy - 5 + dy, 6, Some(sim::nav::tile_of(start)))
            .expect("a passable tile near the far target");
        sim.issue(move_to(0, vec![id], sim::nav::centre(tile)));
    }

    // Step past COMMAND_DELAY before testing for completion: on the tick the
    // order is issued every unit is still Idle, and a completion check here
    // would pass instantly without the order ever having been applied. These
    // are also the ticks in which the whole crowd plans at once, which is
    // when the budget is actually contended.
    let mut deferred_total = 0u64;
    for _ in 0..=sim::COMMAND_DELAY {
        sim.step();
        deferred_total += sim.stats().path_deferred as u64;
    }
    let walking_now = ids
        .iter()
        .filter(|&&id| sim.world().order[index_of(&sim, id)] != Order::Idle)
        .count();
    assert!(
        walking_now >= 40,
        "the move orders did not reach the crowd: only {walking_now} of {} are \
         under way",
        ids.len()
    );

    for _ in 0..20 * 300 {
        sim.step();
        deferred_total += sim.stats().path_deferred as u64;
        if ids
            .iter()
            .all(|&id| sim.world().order[index_of(&sim, id)] == Order::Idle)
        {
            break;
        }
    }

    assert!(
        deferred_total > 0,
        "the budget was never reached, so this test proves nothing about it — \
         order more distinct destinations or check DESTINATIONS_PER_TICK"
    );
    // Every unit must have been served in the end: deferral delays a request,
    // it does not discard it. `Order::Idle` here means the move finished, one
    // way or another; what must not happen is a unit left waiting forever.
    for &id in &ids {
        let i = index_of(&sim, id);
        assert_eq!(
            sim.world().order[i],
            Order::Idle,
            "a unit was still holding its move order after 300 s; a deferred \
             path request must wait a tick, not be dropped"
        );
        assert!(
            sim.world().nav[i].is_none(),
            "and its nav state should have been cleared"
        );
    }
}
