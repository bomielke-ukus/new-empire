//! Scenarios (`docs/02` §13, `docs/07` D35): a match set up as written,
//! decided by the player's objectives, and driven by triggers.

mod common;

use common::*;
use sim::scenario::{
    Action, Area, Condition, Control, DrawnMap, Goal, Objective, ObjectiveStatus, Placement,
    Scenario, ScenarioMap, Side, Trigger,
};
use sim::{kinds, Age, Event, Outcome, Simulation, Victory};

/// A small drawn map: grass, a strip of forest along the top, a pond.
fn drawn() -> ScenarioMap {
    let mut terrain = vec!["g".repeat(32); 32];
    terrain[1] = "f".repeat(32);
    for row in terrain.iter_mut().take(24).skip(20) {
        row.replace_range(20..26, "wwwwww");
    }
    ScenarioMap::Drawn(DrawnMap {
        terrain,
        heights: Vec::new(),
    })
}

fn side(name: &str, control: Control) -> Side {
    Side {
        name: name.into(),
        control,
        civ: None,
        age: Age::Stone,
        stockpile: [0; 4],
        techs: Vec::new(),
        start: None,
    }
}

fn place(kind: &str, owner: u8, at: (i32, i32)) -> Placement {
    Placement {
        kind: kind.into(),
        owner,
        at,
        count: 1,
        tag: None,
    }
}

fn objective(id: &str, goal: Goal) -> Objective {
    Objective {
        id: id.into(),
        text: format!("Do {id}"),
        goal,
        hidden: false,
        optional: false,
    }
}

fn trigger(when: Vec<Condition>, then: Vec<Action>) -> Trigger {
    Trigger {
        id: None,
        when,
        then,
        repeat: false,
    }
}

/// The player with a Town Center and three villagers, and a scripted side
/// with a clubman far off.
fn base() -> Scenario {
    Scenario {
        title: "The Two Lands".into(),
        briefing: Vec::new(),
        id: "test/base".into(),
        seed: None,
        map: drawn(),
        sides: vec![
            side("Thinis", Control::Player),
            side("Raiders", Control::Scripted),
        ],
        standard_start: false,
        placements: vec![
            place("town_center", 0, (4, 8)),
            Placement {
                count: 3,
                tag: Some("first".into()),
                ..place("villager", 0, (6, 13))
            },
            Placement {
                tag: Some("raider".into()),
                ..place("clubman", 1, (28, 28))
            },
            place("berry_bush", kinds::GAIA, (10, 10)),
        ],
        objectives: vec![objective("food", Goal::Scripted)],
        triggers: Vec::new(),
        skirmish_victories: false,
    }
}

fn start(sc: &Scenario) -> Simulation {
    assert_eq!(sc.problems(), Vec::<String>::new());
    Simulation::new(3, sc.config())
}

fn seconds(sim: &mut Simulation, s: u32) {
    run(sim, s * 20);
}

/// REQ: GD-CAMP-01
#[test]
fn a_scenario_sets_the_match_up_as_written() {
    let mut sc = base();
    sc.sides[0].age = Age::Bronze;
    sc.sides[0].civ = Some(sim::Civ::Egyptians);
    sc.sides[0].stockpile = [111, 222, 33, 44];
    sc.sides[0].techs = vec!["Woodworking".into()];
    let sim = start(&sc);
    assert_eq!(sim.map().width(), 32);
    assert_eq!(sim.map().terrain(22, 21), sim::Terrain::ShallowWater);
    // A drawn forest has its trees.
    assert_eq!(owned(&sim, kinds::GAIA, kinds::TREE).len(), 32);
    assert_eq!(owned(&sim, 0, kinds::TOWN_CENTER).len(), 1);
    assert_eq!(owned(&sim, 0, kinds::VILLAGER).len(), 3);
    assert_eq!(owned(&sim, 1, kinds::CLUBMAN).len(), 1);
    assert_eq!(owned(&sim, kinds::GAIA, kinds::BERRY_BUSH).len(), 1);
    let p = sim.player(0).unwrap();
    assert_eq!(p.age, Age::Bronze, "the ages before it applied");
    assert!(p.has_researched(sim::tech::by_name("woodworking").unwrap()));
    assert_eq!(p.stockpile, [111, 222, 33, 44]);
    assert_eq!(sim.civ(0), Some(sim::Civ::Egyptians));
    assert_eq!(
        sim.starts()[0],
        (4, 8),
        "the side's Town Center is its start"
    );
    assert_eq!(sim.objectives().len(), 1);
    // A generated map without a standard start keeps only Gaia's.
    let mut sc = base();
    sc.map = ScenarioMap::Generated {
        kind: sim::MapKind::Inland,
        size: 64,
    };
    sc.placements.clear();
    let sim = start(&sc);
    assert!(owned(&sim, 0, kinds::TOWN_CENTER).is_empty());
    assert!(!owned(&sim, kinds::GAIA, kinds::TREE).is_empty());
    sc.standard_start = true;
    let sim = start(&sc);
    assert_eq!(owned(&sim, 0, kinds::TOWN_CENTER).len(), 1);
}

/// REQ: GD-CAMP-02
#[test]
fn every_objective_done_wins_and_one_failed_loses() {
    // Food in store, then a house standing: both needed.
    let mut sc = base();
    sc.objectives = vec![
        objective(
            "food",
            Goal::Stockpile {
                resource: kinds::Resource::Food,
                amount: 100,
            },
        ),
        objective(
            "house",
            Goal::Have {
                kind: "House".into(),
                count: 1,
            },
        ),
        Objective {
            optional: true,
            ..objective("extra", Goal::Scripted)
        },
    ];
    sc.triggers = vec![
        trigger(
            vec![Condition::After(2)],
            vec![Action::Give {
                owner: 0,
                resource: kinds::Resource::Food,
                amount: 100,
            }],
        ),
        trigger(
            vec![Condition::Done("food".into())],
            vec![Action::Place(place("house", 0, (12, 4)))],
        ),
    ];
    let mut sim = start(&sc);
    seconds(&mut sim, 2);
    assert_eq!(sim.outcome(), None);
    let mut done = 0;
    for _ in 0..3 * 20 {
        sim.step();
        done += sim
            .events()
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Objective {
                        status: ObjectiveStatus::Done,
                        ..
                    }
                )
            })
            .count();
    }
    assert_eq!(done, 2, "each objective's completion is news");
    assert_eq!(
        sim.outcome(),
        Some(&Outcome::Won),
        "the optional one is not needed"
    );
    assert_eq!(sim.victory(), Some((0, Victory::Objectives)));
    assert!(sim.over());

    // A failed objective loses, and says which.
    let mut sc = base();
    sc.triggers = vec![trigger(
        vec![Condition::After(1)],
        vec![Action::Fail("food".into())],
    )];
    let mut sim = start(&sc);
    seconds(&mut sim, 2);
    assert_eq!(
        sim.outcome(),
        Some(&Outcome::Lost("Failed: Do food".into()))
    );
    assert_eq!(sim.winner(), Some(kinds::GAIA));
}

/// REQ: GD-CAMP-02
#[test]
fn the_skirmish_victories_hold_only_when_the_scenario_says_so() {
    // The scripted side has nothing at the start: no conquest for it.
    let mut sc = base();
    sc.placements.retain(|p| p.owner != 1);
    let mut sim = start(&sc);
    seconds(&mut sim, 3);
    assert!(!sim.over(), "a side that has not arrived is not beaten");
    assert_eq!(sim.victory(), None);
    sc.skirmish_victories = true;
    let mut sim = start(&sc);
    seconds(&mut sim, 3);
    assert_eq!(sim.victory(), Some((0, Victory::Conquest)));
    // And with nothing of the player's left, it is lost.
    let mut sc = base();
    sc.placements.retain(|p| p.owner != 0);
    let mut sim = start(&sc);
    seconds(&mut sim, 2);
    assert!(matches!(sim.outcome(), Some(Outcome::Lost(_))));
}

/// REQ: GD-CAMP-03
#[test]
fn triggers_narrate_show_objectives_and_wait_on_each_other() {
    let mut sc = base();
    sc.objectives.push(Objective {
        hidden: true,
        ..objective("later", Goal::Scripted)
    });
    sc.triggers = vec![
        Trigger {
            id: Some("hello".into()),
            ..trigger(Vec::new(), vec![Action::Say("The Nile rises.".into())])
        },
        trigger(
            vec![Condition::Fired("hello".into()), Condition::After(3)],
            vec![Action::Show("later".into())],
        ),
        Trigger {
            repeat: true,
            ..trigger(
                vec![Condition::After(5)],
                vec![Action::Give {
                    owner: 0,
                    resource: kinds::Resource::Wood,
                    amount: 10,
                }],
            )
        },
    ];
    let mut sim = start(&sc);
    sim.step();
    let said: Vec<_> = sim
        .events()
        .iter()
        .filter_map(|e| match *e {
            Event::Said { trigger, action } => sim.line(trigger, action),
            _ => None,
        })
        .collect();
    assert_eq!(said, ["The Nile rises."]);
    assert_eq!(sim.objectives().len(), 1, "the hidden one is not shown");
    seconds(&mut sim, 3);
    assert_eq!(sim.objectives().len(), 2);
    assert_eq!(sim.objectives()[1].status, ObjectiveStatus::Open);
    // A repeating trigger happens each second it holds.
    let wood = sim.player(0).unwrap().stockpile[1];
    seconds(&mut sim, 4);
    assert!(sim.player(0).unwrap().stockpile[1] >= wood + 30);
}

/// REQ: GD-CAMP-03
#[test]
fn a_scripted_side_attacks_and_a_tagged_unit_gone_is_noticed() {
    // Five raiders sent at the player's villagers, who are four axemen.
    let mut sc = base();
    sc.placements.push(Placement {
        count: 4,
        ..place("clubman", 1, (26, 26))
    });
    sc.placements.push(Placement {
        count: 6,
        ..place("axeman", 0, (8, 14))
    });
    let home = Area {
        from: (2, 8),
        to: (14, 18),
    };
    sc.objectives = vec![objective(
        "hold",
        Goal::Destroy {
            owner: 1,
            kind: None,
        },
    )];
    sc.triggers = vec![
        trigger(
            vec![Condition::After(1)],
            vec![Action::Attack {
                owner: 1,
                to: home.middle(),
            }],
        ),
        Trigger {
            id: Some("arrived".into()),
            ..trigger(
                vec![Condition::Inside {
                    owner: 1,
                    area: home,
                    kind: None,
                    count: 1,
                }],
                vec![Action::Say("They are here.".into())],
            )
        },
        trigger(
            vec![Condition::Gone("raider".into())],
            vec![Action::Say("Their leader has fallen.".into())],
        ),
    ];
    let mut sim = start(&sc);
    let mut heard = Vec::new();
    for _ in 0..240 * 20 {
        sim.step();
        for e in sim.events() {
            if let Event::Said { trigger, action } = *e {
                heard.push(sim.line(trigger, action).unwrap().to_string());
            }
        }
        if sim.over() {
            break;
        }
    }
    assert_eq!(heard, ["They are here.", "Their leader has fallen."]);
    assert_eq!(sim.outcome(), Some(&Outcome::Won), "every raider dead");
}

/// A trigger may wait on any one of several things, or on something not
/// holding: a raid that comes when the army is ready or at a set time,
/// whichever is first, and once.
///
/// REQ: GD-CAMP-03
#[test]
fn a_trigger_waits_on_any_of_several_or_on_one_not_holding() {
    let mut sc = base();
    sc.triggers = vec![
        Trigger {
            id: Some("raid".into()),
            ..trigger(
                vec![Condition::Any(vec![
                    Condition::Has {
                        owner: 0,
                        kind: Some("Clubman".into()),
                        count: 2,
                    },
                    Condition::After(4),
                ])],
                vec![Action::Give {
                    owner: 0,
                    resource: kinds::Resource::Gold,
                    amount: 100,
                }],
            )
        },
        Trigger {
            repeat: true,
            ..trigger(
                vec![Condition::Not(Box::new(Condition::Fired("raid".into())))],
                vec![Action::Give {
                    owner: 0,
                    resource: kinds::Resource::Stone,
                    amount: 7,
                }],
            )
        },
    ];
    // Ready early: the first branch.
    let mut sim = start(&sc);
    sim.issue(spawn(0, kinds::CLUBMAN, sim::Vec2Fx::from_int(8, 8)));
    sim.issue(spawn(0, kinds::CLUBMAN, sim::Vec2Fx::from_int(9, 8)));
    seconds(&mut sim, 2);
    let p = sim.player(0).unwrap();
    assert_eq!(p.stockpile[3], 100, "the raid came for the army");
    let stone = p.stockpile[2];
    assert!(stone > 0, "and the other waited on it not having come");
    seconds(&mut sim, 6);
    let p = sim.player(0).unwrap();
    assert_eq!(p.stockpile[3], 100, "once only");
    assert_eq!(
        p.stockpile[2], stone,
        "it no longer holds once the raid came"
    );
    // Not ready: the clock.
    let mut sim = start(&sc);
    seconds(&mut sim, 3);
    assert_eq!(sim.player(0).unwrap().stockpile[3], 0);
    seconds(&mut sim, 2);
    assert_eq!(
        sim.player(0).unwrap().stockpile[3],
        100,
        "the raid came on time"
    );
}

/// A trigger may wait so long after another first happened, however
/// late that was.
///
/// REQ: GD-CAMP-03
#[test]
fn a_trigger_waits_so_long_after_another() {
    let mut sc = base();
    sc.triggers = vec![
        Trigger {
            id: Some("founded".into()),
            ..trigger(
                vec![Condition::Has {
                    owner: 0,
                    kind: Some("Clubman".into()),
                    count: 1,
                }],
                Vec::new(),
            )
        },
        trigger(
            vec![Condition::Since {
                trigger: "founded".into(),
                seconds: 5,
            }],
            vec![Action::Give {
                owner: 0,
                resource: kinds::Resource::Gold,
                amount: 100,
            }],
        ),
    ];
    let mut sim = start(&sc);
    seconds(&mut sim, 10);
    assert_eq!(
        sim.player(0).unwrap().stockpile[3],
        0,
        "nothing founded yet"
    );
    sim.issue(spawn(0, kinds::CLUBMAN, sim::Vec2Fx::from_int(8, 8)));
    seconds(&mut sim, 5);
    assert_eq!(
        sim.player(0).unwrap().stockpile[3],
        0,
        "not five seconds on"
    );
    seconds(&mut sim, 2);
    assert_eq!(sim.player(0).unwrap().stockpile[3], 100);
    let mut bad = base();
    bad.triggers.push(trigger(
        vec![Condition::Since {
            trigger: "nowhere".into(),
            seconds: 1,
        }],
        vec![Action::Win],
    ));
    assert!(bad.problems().join("\n").contains("no trigger \"nowhere\""));
}

/// REQ: GD-CAMP-04
#[test]
fn a_scenario_is_checked_when_it_is_loaded() {
    assert!(base().problems().is_empty());
    let mut sc = base();
    sc.sides[1].control = Control::Player;
    sc.placements.push(place("chariot of fire", 0, (3, 3)));
    sc.placements.push(place("house", 5, (99, 3)));
    sc.objectives.push(objective("food", Goal::Scripted));
    sc.triggers.push(trigger(
        vec![
            Condition::Done("nothing".into()),
            Condition::Any(vec![
                Condition::Gone("nobody".into()),
                Condition::Not(Box::new(Condition::Fired("never".into()))),
            ]),
            Condition::Any(Vec::new()),
        ],
        vec![Action::Show("missing".into())],
    ));
    let problems = sc.problems().join("\n");
    for expected in [
        "only it, is the player's",
        "no kind called \"chariot of fire\"",
        "no side 5",
        "off the map",
        "\"food\" is defined twice",
        "no objective \"nothing\"",
        "nothing tagged \"nobody\"",
        "no trigger \"never\"",
        "Any of nothing never holds",
        "no objective \"missing\"",
    ] {
        assert!(
            problems.contains(expected),
            "{expected:?} not in:\n{problems}"
        );
    }
    let mut sc = base();
    if let ScenarioMap::Drawn(d) = &mut sc.map {
        d.terrain[5].push('g');
    }
    assert!(sc.problems().join("\n").contains("map row 5"));
}

/// REQ: GD-CAMP-01
#[test]
fn a_scenario_reads_from_its_file_and_replays_exactly() {
    let text = r#"(
        map: Drawn((terrain: [
            "ffffffffffffffffffffffffffffffff",
            "gggggggggggggggggggggggggggggggg", "gggggggggggggggggggggggggggggggg",
            "gggggggggggggggggggggggggggggggg", "gggggggggggggggggggggggggggggggg",
            "gggggggggggggggggggggggggggggggg", "gggggggggggggggggggggggggggggggg",
            "gggggggggggggggggggggggggggggggg", "gggggggggggggggggggggggggggggggg",
            "gggggggggggggggggggggggggggggggg", "gggggggggggggggggggggggggggggggg",
            "gggggggggggggggggggggggggggggggg", "gggggggggggggggggggggggggggggggg",
            "gggggggggggggggggggggggggggggggg", "gggggggggggggggggggggggggggggggg",
            "gggggggggggggggggggggggggggggggg", "gggggggggggggggggggggggggggggggg",
        ])),
        sides: [
            (name: "Thinis", control: Player, civ: Some(Egyptians)),
            (name: "Nubians", control: Computer(1)),
        ],
        placements: [
            (kind: "Town Center", owner: 0, at: (4, 6)),
            (kind: "villager", owner: 0, at: (6, 11), count: 3),
            (kind: "Town Center", owner: 1, at: (26, 6)),
        ],
        objectives: [
            (id: "wood", text: "Gather 100 wood", goal: Stockpile(resource: Wood, amount: 100)),
        ],
        triggers: [
            (then: [Say("Gather wood from the forest to the north.")]),
        ],
    )"#;
    let sc: Scenario = ron::from_str(text).expect("the file reads");
    assert!(sc.problems().is_empty(), "{:?}", sc.problems());
    assert_eq!(sc.sides[1].control, Control::Computer(1));
    let run_for = |ticks: u32| {
        let mut sim = Simulation::new(9, sc.config());
        run(&mut sim, ticks);
        (sim.state_hash(), sim)
    };
    let (a, sim) = run_for(100);
    let (b, _) = run_for(100);
    assert_eq!(a, b, "a scenario plays the same twice");
    // A save carries it, and the match carries on as it would have.
    let saved = ron::to_string(&sim).unwrap();
    let mut loaded: Simulation = ron::from_str(&saved).unwrap();
    assert_eq!(loaded.state_hash(), a);
    let mut sim = sim;
    run(&mut sim, 40);
    run(&mut loaded, 40);
    assert_eq!(loaded.state_hash(), sim.state_hash());
}
