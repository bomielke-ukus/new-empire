//! The historical campaigns (`docs/07` D35), played through by the plain
//! bot of `common::bot`: each battle is won by doing the obvious thing
//! well enough, and lost by not doing the thing the scenario turns on.

mod common;

use common::bot::*;
use common::*;
use sim::kinds;
use sim::scenario::ObjectiveStatus;
use sim::{Command, CommandKind, Outcome, Vec2Fx};

fn soldiers(b: &Bot) -> Vec<sim::EntityId> {
    let w = b.sim.world();
    w.slots()
        .filter(|s| {
            let i = s.index();
            let info = kinds::info(w.kind[i]);
            w.owner[i] == ME
                && w.dying[i] == 0
                && info.mobile
                && info.combat.attack > 0
                && !kinds::gathers(w.kind[i])
        })
        .map(|s| w.id_at(s))
        .collect()
}

/// A group holds a post: those idle take on the nearest enemy within eight
/// tiles of it, or walk back to it.
fn hold(b: &mut Bot, group: &[sim::EntityId], post: (i32, i32)) {
    let idle = b.sim.idle_soldiers(ME);
    let at = Vec2Fx::from_int(post.0, post.1);
    let near = {
        let w = b.sim.world();
        w.slots()
            .filter(|t| w.owner[t.index()] == 1 && w.dying[t.index()] == 0)
            .map(|t| (at.distance_sq_raw(w.pos[t.index()]), w.id_at(t)))
            .filter(|(d, _)| *d <= (8u64 * 65536).pow(2))
            .min_by_key(|(d, _)| *d)
            .map(|(_, id)| id)
    };
    let mine: Vec<_> = group
        .iter()
        .copied()
        .filter(|id| idle.contains(id))
        .collect();
    if mine.is_empty() {
        return;
    }
    match near {
        Some(target) => b.issue(CommandKind::Attack { ids: mine, target }),
        None => {
            let away: Vec<_> = mine
                .into_iter()
                .filter(|&id| pos_of(&b.sim, id).distance_sq_raw(at) > (3u64 * 65536).pow(2))
                .collect();
            if !away.is_empty() {
                move_to(b, away, post);
            }
        }
    }
}

fn move_to(b: &mut Bot, ids: Vec<sim::EntityId>, to: (i32, i32)) {
    b.issue(CommandKind::Move {
        ids,
        target: Vec2Fx::from_int(to.0, to.1),
    });
}

/// Marathon: the army on the plain is beaten by the hoplites going in
/// together, and the march home beats the fleet round the cape.
///
/// REQ: GD-CAMP-03
#[test]
fn marathon_is_won_on_the_plain_and_by_the_march_home() {
    let mut b = Bot::new("persian-wars", "marathon");
    // The Plataeans come first.
    run(&mut b.sim, 30 * SECOND);
    let army = soldiers(&b);
    assert!(army.len() >= 22, "{}", army.len());
    b.attack_move(army, (71, 36));
    b.fight(5 * MINUTE, "plain", &[1], |b| {
        b.status("plain") == ObjectiveStatus::Done
    });
    let won_at = b.minutes();
    let hoplites = b.mine(kinds::HOPLITE).len();
    eprintln!("marathon: the plain won at {won_at:.1} minutes, {hoplites} hoplites stand");
    assert!(hoplites >= 9, "the hoplites win it clearly");
    // Home by the road, without stopping to fight.
    let home = b.mine(kinds::HOPLITE);
    move_to(&mut b, home, (13, 80));
    b.until(4 * MINUTE, "athens", |b| b.sim.outcome().is_some());
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
    // How far the fleet still had to go.
    let fleet = owned(&b.sim, 2, kinds::WAR_GALLEY);
    let left = fleet
        .iter()
        .map(|&s| {
            let p = pos_of(&b.sim, s);
            (p.x.floor() - 29).abs() + (p.y.floor() - 85).abs()
        })
        .min()
        .unwrap_or(0);
    eprintln!(
        "marathon: Athens reached at {:.1} minutes, the fleet {left} tiles short",
        b.minutes()
    );
    assert!(left >= 15, "the race is won with room to spare");
}

/// Marathon is lost by dawdling after the battle: the fleet gets there.
///
/// REQ: GD-CAMP-03
#[test]
fn marathon_is_lost_if_the_army_does_not_march_home() {
    let mut b = Bot::new("persian-wars", "marathon");
    run(&mut b.sim, 30 * SECOND);
    let army = soldiers(&b);
    b.attack_move(army, (71, 36));
    b.fight(5 * MINUTE, "plain", &[1], |b| {
        b.status("plain") == ObjectiveStatus::Done
    });
    b.until(5 * MINUTE, "the fleet", |b| b.sim.outcome().is_some());
    assert!(matches!(b.sim.outcome(), Some(Outcome::Lost(_))));
}

/// Thermopylae: standing at the wall is not enough, since the Persian
/// archers shoot from beyond a hoplite's sight and the Immortals come
/// round behind; charging what comes, and turning to meet the Immortals,
/// holds it.
///
/// REQ: GD-CAMP-03
#[test]
fn thermopylae_is_held_by_fighting_what_comes_and_turning_to_the_immortals() {
    // Standing still at the wall.
    let mut b = Bot::new("persian-wars", "thermopylae");
    b.until(11 * MINUTE, "passive", |b| b.sim.outcome().is_some());
    eprintln!(
        "thermopylae standing still: {:?} at {:.1} minutes",
        b.sim.outcome(),
        b.minutes()
    );
    assert!(matches!(b.sim.outcome(), Some(Outcome::Lost(_))));
    // Fighting: each group holds a post, takes on what comes near it and
    // goes back to it; at the warning, a reserve goes to the mountain
    // path to meet the Immortals.
    let mut b = Bot::new("persian-wars", "thermopylae");
    let hoplites = b.mine(kinds::HOPLITE);
    let mut front: Vec<_> = hoplites[4..].to_vec();
    let mut reserve: Vec<_> = hoplites[..4].to_vec();
    for k in [kinds::SPEARMAN, kinds::BOWMAN, kinds::SLINGER] {
        reserve.extend(b.mine(k));
    }
    let mut posts = vec![(front.clone(), (35, 32)), (reserve.clone(), (35, 38))];
    while b.sim.outcome().is_none() {
        assert!(b.sim.tick() < 11 * MINUTE as u64, "not decided");
        if b.sim.tick() == (6 * MINUTE + 40 * SECOND) as u64 {
            posts[1].1 = (14, 57);
        }
        if b.sim.tick().is_multiple_of(2 * SECOND as u64) {
            for (group, post) in &mut posts {
                group.retain(|&id| b.sim.world().slot(id).is_some());
                hold(&mut b, group, *post);
            }
            front.retain(|&id| b.sim.world().slot(id).is_some());
            reserve.retain(|&id| b.sim.world().slot(id).is_some());
        }
        b.sim.step();
    }
    eprintln!(
        "thermopylae fighting: {:?} at {:.1} minutes, {} soldiers left",
        b.sim.outcome(),
        b.minutes(),
        soldiers(&b).len()
    );
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
}

/// Salamis: the Greek fleet meets each squadron as it comes and sinks
/// them all.
///
/// REQ: GD-CAMP-03
#[test]
fn salamis_is_won_by_meeting_each_squadron() {
    let mut b = Bot::new("persian-wars", "salamis");
    b.fight(12 * MINUTE, "fleet", &[1], |b| b.sim.outcome().is_some());
    let ships = b.mine(kinds::WAR_GALLEY).len() + b.mine(kinds::ARCHER_SHIP).len();
    eprintln!(
        "salamis: {:?} at {:.1} minutes, {ships} ships afloat",
        b.sim.outcome(),
        b.minutes()
    );
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
}

/// Plataea: Mardonius' tent burned wins it, the Greek camp burned loses
/// it. (That the computer plays him is the app's test: the opponent is
/// not the simulation's.)
///
/// REQ: GD-CAMP-03
#[test]
fn plataea_is_won_at_mardonius_tent_and_lost_at_the_greek_camp() {
    let mut b = Bot::new("persian-wars", "plataea");
    let tent = owned(&b.sim, 1, kinds::GOVERNMENT_CENTRE)[0];
    b.sim.issue(Command {
        player: 1,
        kind: CommandKind::Despawn { id: tent },
    });
    run(&mut b.sim, 2 * SECOND);
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
    let mut b = Bot::new("persian-wars", "plataea");
    let camp = owned(&b.sim, 0, kinds::TOWN_CENTER)[0];
    b.issue(CommandKind::Despawn { id: camp });
    run(&mut b.sim, 2 * SECOND);
    assert!(matches!(b.sim.outcome(), Some(Outcome::Lost(_))));
}

/// The Cupbearer: Agade grown, armed and advanced while Sargon and his
/// clubmen meet Kish's raids at home.
///
/// REQ: GD-CAMP-03
#[test]
fn the_cupbearer_is_won_by_growing_agade_and_meeting_kish() {
    use sim::kinds::Resource;
    let mut b = Bot::new("sargon", "cupbearer");
    b.assign(Resource::Food);
    b.set_job(3, Resource::Wood);
    let tc = b.mine(kinds::TOWN_CENTER)[0];
    let home = b.tile_of(tc);
    let sargon = b.mine(kinds::SWORDSMAN);
    b.issue(CommandKind::Garrison {
        ids: sargon,
        building: tc,
    });
    // Sargon shelters in the Town Center; the clubmen and what the
    // barracks trains meet the raids.
    let mut guards: Vec<_> = b.mine(kinds::CLUBMAN);
    let mut stage = 0;
    while b.sim.outcome().is_none() {
        assert!(
            b.sim.tick() < 25 * MINUTE as u64,
            "not decided: {:?}",
            b.sim
                .objectives()
                .iter()
                .map(|o| (o.text.to_string(), o.status))
                .collect::<Vec<_>>()
        );
        let t = b.sim.tick();
        if t.is_multiple_of(2 * SECOND as u64) {
            b.work();
            guards.retain(|&id| b.sim.world().slot(id).is_some());
            for c in b.mine(kinds::CLUBMAN) {
                if !guards.contains(&c) {
                    guards.push(c);
                }
            }
            hold(&mut b, &guards, (home.0 + 1, home.1 - 6));
        }
        if t.is_multiple_of(5 * SECOND as u64) {
            let p = b.sim.player(ME).unwrap().clone();
            let (food, wood) = (p.stockpile[0], p.stockpile[1]);
            let vills = b.mine(kinds::VILLAGER).len();
            let queued = |b: &Bot| {
                b.sim.world().production[index_of(&b.sim, tc)]
                    .as_ref()
                    .map_or(0, |p| p.queue.len())
            };
            if p.pop + 2 >= p.pop_cap && wood >= 30 && stage < 8 {
                b.build(kinds::HOUSE, (home.0 - 4 - 2 * stage, home.1 - 4), 1);
                stage += 1;
            } else if vills < 16 && food >= 50 && queued(&b) < 2 && p.pop < p.pop_cap {
                b.train(kinds::TOWN_CENTER, kinds::VILLAGER, 1);
            } else if b.mine(kinds::STOREHOUSE).is_empty()
                && owned(&b.sim, ME, kinds::STOREHOUSE).is_empty()
                && wood >= 100
            {
                b.build(kinds::STOREHOUSE, (34, 30), 1);
            } else if owned(&b.sim, ME, kinds::BARRACKS).is_empty() && wood >= 125 {
                b.build(kinds::BARRACKS, (home.0 + 5, home.1 - 3), 1);
            } else if !b.mine(kinds::BARRACKS).is_empty()
                && food >= 150
                && b.mine(kinds::CLUBMAN).len() < 6
            {
                b.train(kinds::BARRACKS, kinds::CLUBMAN, 1);
            } else if vills >= 14
                && food >= 400
                && b.mine(kinds::BARRACKS).len() + b.mine(kinds::STOREHOUSE).len() >= 2
                && queued(&b) == 0
            {
                b.issue(CommandKind::Research {
                    building: tc,
                    tech: sim::tech::AGE_TOOL,
                });
            }
            if vills >= 10 {
                b.set_job(6, Resource::Wood);
            }
        }
        b.sim.step();
    }
    eprintln!(
        "cupbearer: {:?} at {:.1} minutes, {} villagers",
        b.sim.outcome(),
        b.minutes(),
        b.mine(kinds::VILLAGER).len()
    );
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
}

/// Lugal-zage-si: Uruk's Town Center gone wins it; Sargon gone loses it.
///
/// REQ: GD-CAMP-03
#[test]
fn uruk_is_won_at_its_town_center_and_lost_with_sargon() {
    let mut b = Bot::new("sargon", "uruk");
    let palace = owned(&b.sim, 1, kinds::TOWN_CENTER)[0];
    b.sim.issue(Command {
        player: 1,
        kind: CommandKind::Despawn { id: palace },
    });
    run(&mut b.sim, 2 * SECOND);
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
    let mut b = Bot::new("sargon", "uruk");
    let sargon = b.mine(kinds::SWORDSMAN)[0];
    b.issue(CommandKind::Despawn { id: sargon });
    run(&mut b.sim, 2 * SECOND);
    assert_eq!(
        b.sim.outcome(),
        Some(&Outcome::Lost(
            "Sargon has fallen, and Akkad with him".into()
        ))
    );
}

/// Washing Weapons in the Sea: the army takes Ur, then Lagash, its stone
/// throwers on the walls and towers, and marches to the shore.
///
/// REQ: GD-CAMP-03
#[test]
fn the_gulf_is_won_by_taking_ur_and_lagash_and_reaching_the_sea() {
    let mut b = Bot::new("sargon", "gulf");
    let army = soldiers(&b);
    let sargon = b.mine(kinds::SWORDSMAN)[0];
    let without: Vec<_> = army.iter().copied().filter(|&id| id != sargon).collect();
    for (what, at) in [("ur", (34, 72)), ("lagash", (68, 38))] {
        let army: Vec<_> = without
            .iter()
            .copied()
            .filter(|&id| b.sim.world().slot(id).is_some())
            .collect();
        b.attack_move(army, at);
        let start = b.sim.tick();
        while b.status(what) != ObjectiveStatus::Done {
            assert!(
                b.sim.tick() - start < 8 * MINUTE as u64,
                "{what} not taken; {} soldiers left",
                soldiers(&b).len()
            );
            if b.sim.tick().is_multiple_of(3 * SECOND as u64) {
                let mut army: Vec<_> = without
                    .iter()
                    .copied()
                    .filter(|&id| b.sim.world().slot(id).is_some())
                    .collect();
                army.retain(|id| b.sim.idle_soldiers(ME).contains(id));
                // Idle ones go for what is left of the city: its soldiers,
                // then its towers, then its Town Center; not its walls.
                let target = {
                    let w = b.sim.world();
                    let c = Vec2Fx::from_int(at.0, at.1);
                    let rank = |k: u16| {
                        if kinds::info(k).mobile {
                            0
                        } else if k == kinds::WATCH_TOWER {
                            1
                        } else if k == kinds::TOWN_CENTER {
                            2
                        } else {
                            9
                        }
                    };
                    w.slots()
                        .filter(|t| w.owner[t.index()] == 1 && w.dying[t.index()] == 0)
                        .map(|t| {
                            (
                                rank(w.kind[t.index()]),
                                c.distance_sq_raw(w.pos[t.index()]),
                                w.id_at(t),
                            )
                        })
                        .filter(|(r, d, _)| *r < 9 && *d <= (14u64 * 65536).pow(2))
                        .min_by_key(|(r, d, _)| (*r, *d))
                        .map(|(_, _, id)| id)
                };
                if let (Some(target), false) = (target, army.is_empty()) {
                    b.issue(CommandKind::Attack { ids: army, target });
                }
            }
            b.sim.step();
        }
        eprintln!(
            "gulf: {what} taken at {:.1} minutes, {} soldiers left",
            b.minutes(),
            soldiers(&b).len()
        );
    }
    let army = soldiers(&b);
    move_to(&mut b, army, (68, 73));
    b.until(4 * MINUTE, "the sea", |b| b.sim.outcome().is_some());
    eprintln!("gulf: {:?} at {:.1} minutes", b.sim.outcome(), b.minutes());
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
}

/// King of the Four Quarters: the army holds the gates for ten minutes,
/// then goes out and destroys the four camps.
///
/// REQ: GD-CAMP-03
#[test]
fn the_four_quarters_are_won_by_holding_agade_then_breaking_the_camps() {
    let mut b = Bot::new("sargon", "four-quarters");
    let sargon = b.mine(kinds::SWORDSMAN)[0];
    let tc = b.mine(kinds::TOWN_CENTER)[0];
    b.issue(CommandKind::Garrison {
        ids: vec![sargon],
        building: tc,
    });
    // A siege is ten minutes long: the town keeps working and the
    // Barracks keeps training, and what it trains goes to the posts.
    while b.status("hold") != ObjectiveStatus::Done {
        assert!(b.sim.outcome().is_none(), "{:?}", b.sim.outcome());
        if b.sim.tick().is_multiple_of(2 * SECOND as u64) {
            let army: Vec<_> = soldiers(&b)
                .into_iter()
                .filter(|&id| id != sargon)
                .collect();
            hold(&mut b, &army, (44, 40));
            b.work();
        }
        if b.sim.tick().is_multiple_of(20 * SECOND as u64) {
            b.train(kinds::BARRACKS, kinds::SWORDSMAN, 1);
        }
        b.sim.step();
    }
    eprintln!("four quarters: held, {} soldiers left", soldiers(&b).len());
    for camp in [(6, 6), (72, 6), (72, 72), (30, 74)] {
        let army: Vec<_> = soldiers(&b)
            .into_iter()
            .filter(|&id| id != sargon)
            .collect();
        b.attack_move(army, camp);
        b.fight(4 * MINUTE, "a camp", &[1], |b| {
            let w = b.sim.world();
            let c = Vec2Fx::from_int(camp.0, camp.1);
            !w.slots().any(|t| {
                w.owner[t.index()] == 1
                    && w.dying[t.index()] == 0
                    && c.distance_sq_raw(w.pos[t.index()]) <= (10u64 * 65536).pow(2)
            }) || b.sim.outcome().is_some()
        });
    }
    b.fight(4 * MINUTE, "the rest", &[1], |b| b.sim.outcome().is_some());
    eprintln!(
        "four quarters: {:?} at {:.1} minutes",
        b.sim.outcome(),
        b.minutes()
    );
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
}
