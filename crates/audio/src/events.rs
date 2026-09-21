//! The simulation's events as cues (`TA-AUDIO-02`): the presentation
//! never polls the world for something to make a noise about; it reads
//! what happened this tick. Seen through one player's fog, nothing plays
//! for what that player cannot see (`docs/05` §5.2): the fog hides sound
//! as it hides sight.

use crate::Cue;
use sim::{kinds, tech, Class, Event, Simulation, Vec2Fx};

/// A cue and where it is, in tiles, if it is anywhere.
pub type Placed = (Cue, Option<(f32, f32)>);

/// The cues for this tick's events, for `viewer`. `None` is every eye at
/// once, which only a replay allows: it hears the world, and nothing that
/// is one side's news.
pub fn cues(sim: &Simulation, viewer: Option<u8>) -> Vec<Placed> {
    let fog = viewer.and_then(|p| sim.fog(p));
    let world = sim.world();
    let visible = |pos: Vec2Fx| fog.is_none_or(|f| f.visible(pos.x.floor(), pos.y.floor()));
    let mine = |owner: u8| viewer == Some(owner);
    let mut out = Vec::new();
    for e in sim.events() {
        match *e {
            Event::Alarm { player, .. } => {
                if mine(player) {
                    out.push((Cue::Alarm, None));
                }
            }
            Event::Hit { target, pos, .. } => {
                // One's own are always heard: a unit dying is the last
                // thing its side sees of the tile, and the blow that killed
                // it is not in the fog.
                let slot = world.slot(target).map(|s| s.index());
                let own = slot.is_some_and(|i| mine(world.owner[i]));
                if own || visible(pos) {
                    let building = slot.is_some_and(|i| !kinds::info(world.kind[i]).mobile);
                    out.push((Cue::Hit { building }, Some(tile(pos))));
                }
            }
            Event::Death { kind, owner, pos } => {
                if mine(owner) || visible(pos) {
                    let class = kinds::info(kind).class;
                    let class = if kinds::info(kind).mobile {
                        class
                    } else {
                        Class::Building
                    };
                    out.push((Cue::Death(class), Some(tile(pos))));
                }
                if mine(owner) {
                    out.push((Cue::Loss, None));
                }
            }
            Event::Completed { pos, .. } => {
                if visible(pos) {
                    out.push((Cue::Completed, Some(tile(pos))));
                }
            }
            Event::Trained { pos, .. } => {
                if visible(pos) {
                    out.push((Cue::Trained, Some(tile(pos))));
                }
            }
            Event::Deposited { pos, .. } => {
                if visible(pos) {
                    out.push((Cue::Deposited, Some(tile(pos))));
                }
            }
            Event::Work { task, pos, .. } => {
                if visible(pos) {
                    out.push((Cue::Work(task), Some(tile(pos))));
                }
            }
            // A node giving out is seen, not heard: the last swing was.
            Event::Felled { .. } => {}
            Event::Researched { owner, tech: id } => {
                if mine(owner) {
                    let cue = match tech::info(id).and_then(|t| t.advances_age()) {
                        Some(age) => Cue::Fanfare(age),
                        None => Cue::Research,
                    };
                    out.push((cue, None));
                }
            }
        }
    }
    out
}

fn tile(pos: Vec2Fx) -> (f32, f32) {
    (pos.x.raw() as f32 / 65536.0, pos.y.raw() as f32 / 65536.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Command, CommandKind, MapKind, MapSpec, SimConfig, Task};

    fn sim() -> Simulation {
        Simulation::new(
            7,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 64,
                    players: 2,
                },
                starting_stockpile: [5000; 4],
                wander: false,
                ..SimConfig::default()
            },
        )
    }

    fn spawn(sim: &mut Simulation, player: u8, kind: u16, x: i32, y: i32) -> sim::EntityId {
        sim.issue(Command {
            player,
            kind: CommandKind::Spawn {
                kind,
                pos: sim::nav::centre((x, y)),
            },
        });
        for _ in 0..3 {
            sim.step();
        }
        let world = sim.world();
        world
            .slots()
            .filter(|s| world.kind[s.index()] == kind)
            .map(|s| world.id_at(s))
            .last()
            .expect("spawned")
    }

    fn run(sim: &mut Simulation, ticks: u32, viewer: Option<u8>) -> Vec<Cue> {
        let mut out = Vec::new();
        for _ in 0..ticks {
            sim.step();
            out.extend(cues(sim, viewer).into_iter().map(|(c, _)| c));
        }
        out
    }

    /// The cues come from the tick's events and only those: a villager
    /// chopping swings on the beat, a load delivered clinks, a building
    /// finished chimes, a unit trained steps out, a technology done is a
    /// note and an age a fanfare, a fight is heard by whoever can see it
    /// and a loss is the loser's alone. A fight in a player's fog is
    /// silent for that player.
    ///
    /// REQ: TA-AUDIO-02
    #[test]
    fn events_become_cues_and_the_fog_silences_what_it_hides() {
        let mut s = sim();
        // Player 0's economy in one corner.
        let v = spawn(&mut s, 0, kinds::VILLAGER, 8, 8);
        let tree = spawn(&mut s, 0, kinds::TREE, 10, 8);
        spawn(&mut s, 0, kinds::STOREHOUSE, 8, 11);
        s.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: vec![v],
                node: tree,
            },
        });
        let heard = run(&mut s, 900, Some(0));
        let chops = heard
            .iter()
            .filter(|c| **c == Cue::Work(Task::Chop))
            .count();
        assert!(chops >= 20, "{chops} swings in 45 seconds");
        assert!(heard.contains(&Cue::Deposited), "a load delivered");
        assert!(
            !heard.contains(&Cue::Hit { building: false }),
            "nothing fought"
        );

        // A fight in the far corner: player 1's villager hit by an enemy.
        let victim = spawn(&mut s, 1, kinds::VILLAGER, 56, 56);
        let clubs: Vec<_> = (0..3)
            .map(|k| spawn(&mut s, 0, kinds::CLUBMAN, 53 + k, 55))
            .collect();
        s.issue(Command {
            player: 0,
            kind: CommandKind::Attack {
                ids: clubs,
                target: victim,
            },
        });
        let by_0 = run(&mut s, 600, Some(0));
        assert!(
            by_0.contains(&Cue::Hit { building: false }),
            "the attacker's side sees its own clubman's blows"
        );
        // Rewind the same fight for the other views: the recording is the
        // whole match, so replay it and listen as player 1 and as no one.
        let replay = s.replay();
        let mut again = Simulation::new(replay.seed, replay.config.clone());
        let mut by_1 = Vec::new();
        let mut by_all = Vec::new();
        let mut next = 0;
        while again.tick() < replay.ticks {
            while let Some((t, c)) = replay.commands.get(next) {
                if *t != again.tick() {
                    break;
                }
                again.issue(c.clone());
                next += 1;
            }
            again.step();
            by_1.extend(cues(&again, Some(1)).into_iter().map(|(c, _)| c));
            by_all.extend(cues(&again, None).into_iter().map(|(c, _)| c));
        }
        assert!(
            by_1.contains(&Cue::Hit { building: false }),
            "the victim's side hears it"
        );
        assert!(by_1.contains(&Cue::Alarm), "and gets the bell");
        assert!(by_1.contains(&Cue::Loss), "and the loss");
        assert!(by_1.contains(&Cue::Death(Class::Villager)));
        assert!(!by_0.contains(&Cue::Alarm), "the attacker gets no bell");
        assert!(!by_0.contains(&Cue::Loss), "nor a loss");
        assert!(
            !by_1.contains(&Cue::Work(Task::Chop)),
            "player 0's woodline is in player 1's fog: silent"
        );
        assert!(
            by_all.contains(&Cue::Work(Task::Chop)),
            "every eye hears the woodline"
        );
        assert!(by_all.contains(&Cue::Hit { building: false }));
        assert!(!by_all.contains(&Cue::Alarm), "no side's news for no one");
        assert!(!by_all.contains(&Cue::Loss));
    }

    /// Building, training and research each raise their cue, with the
    /// age advance a fanfare rather than a note.
    ///
    /// REQ: TA-AUDIO-02
    #[test]
    fn completion_training_and_research_are_heard() {
        let mut s = sim();
        let v = spawn(&mut s, 0, kinds::VILLAGER, 8, 8);
        let tc = spawn(&mut s, 0, kinds::TOWN_CENTER, 12, 12);
        s.issue(Command {
            player: 0,
            kind: CommandKind::Build {
                kind: kinds::HOUSE,
                x: 8,
                y: 12,
                ids: vec![v],
            },
        });
        s.issue(Command {
            player: 0,
            kind: CommandKind::Train {
                building: tc,
                kind: kinds::VILLAGER,
            },
        });
        let heard = run(&mut s, 1200, Some(0));
        assert!(heard.contains(&Cue::Work(Task::Build)), "hammering");
        assert!(heard.contains(&Cue::Completed), "the house finished");
        assert!(heard.contains(&Cue::Trained), "the villager stepped out");
        // A plain technology is a note; the age, with the two buildings it
        // asks for standing, is a fanfare.
        if let Some(plain) = tech::all().iter().find(|t| {
            t.age == sim::Age::Stone && t.advances_age().is_none() && t.requires.is_empty()
        }) {
            let at = spawn(&mut s, 0, plain.building, 20, 20);
            s.issue(Command {
                player: 0,
                kind: CommandKind::Research {
                    building: at,
                    tech: plain.id,
                },
            });
            let heard = run(&mut s, plain.seconds as u32 * 20 + 40, Some(0));
            assert!(heard.contains(&Cue::Research), "{}: {heard:?}", plain.name);
        }
        spawn(&mut s, 0, kinds::STOREHOUSE, 16, 8);
        spawn(&mut s, 0, kinds::BARRACKS, 16, 16);
        let age = tech::all()
            .iter()
            .find(|t| t.advances_age() == Some(sim::Age::Tool))
            .expect("the Tool Age advance");
        s.issue(Command {
            player: 0,
            kind: CommandKind::Research {
                building: tc,
                tech: age.id,
            },
        });
        let heard = run(&mut s, 2400, Some(0));
        assert!(heard.contains(&Cue::Fanfare(sim::Age::Tool)), "{heard:?}");
        assert!(!heard.contains(&Cue::Research), "an age is not a note");
    }
}
