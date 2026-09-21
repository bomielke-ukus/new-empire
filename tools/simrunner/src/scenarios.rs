//! The named match recipes the corpus and the benchmarks are built from.
//!
//! Keeping them here, rather than as ad-hoc command lines in CI, means the
//! corpus can be regenerated reproducibly (`simrunner record`) and that adding
//! a scenario is a code review rather than a YAML edit nobody reads.
//!
//! These drive the actual systems — gathering, drop-off, construction,
//! training, research and age advances, farms, group pathing — rather than
//! the random spawn-and-move stream the M0 runner used. A corpus that only
//! exercises movement would not notice a change to the economy.

use ai::{Difficulty, Opponent};
use fogged::FoggedView;
use sim::{
    kinds, tech, Command, CommandKind, EntityId, KindId, MapKind, MapSpec, PlayerId, Rally, Replay,
    Rng, SimConfig, Simulation, Source, Timings, Vec2Fx,
};
use std::time::Instant;

/// A reproducible synthetic match.
pub struct Scenario {
    /// File-safe name; the corpus entry is `<name>.ron` + `<name>.golden`.
    pub name: &'static str,
    /// Why this scenario is in the corpus at all.
    pub purpose: &'static str,
    /// Match seed.
    pub seed: u64,
    /// How long to run.
    pub ticks: u64,
    /// Match parameters.
    pub config: SimConfig,
    /// What the scripted players do.
    pub style: Style,
}

/// How the scripted players behave.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// Bounded, mixed-army combat acceptance fixture.
    Battle,
    /// Nobody issues anything. Isolates map generation and idle behaviour —
    /// with `wander` on, the animals still move and the RNG still turns.
    Idle,
    /// Gather, build, train: a plausible opening, played by every player.
    Economy,
    /// Send every mobile unit on long crossings of the map, repeatedly, and
    /// spawn a crowd first if the map did not provide one. This is the
    /// pathfinding and separation load, and it works on a bare `Flat` map,
    /// which has no start kit at all.
    Marching,
    /// Economy plus marching, plus spawned crowds. The busiest thing here.
    Everything,
    /// An economy that builds its way up the ages: construction pulls
    /// villagers off their nodes, research is tried at every building, and
    /// the stockpile is expected to be deep enough to pay for it.
    Ages,
    /// Two hundred soldiers a side, spawned facing each other and sent in:
    /// the "400 units fighting" of `docs/04` §11, and the combat, order
    /// and separation load of §12.
    Melee,
    /// Nobody scripted: every side is a Hard computer opponent, each
    /// handed a crowd at the start so the world is §12-sized from the
    /// first tick. The opponents' thinking is part of what is measured.
    Opponents,
}

fn inland(size: u16, players: u8) -> SimConfig {
    SimConfig {
        map: MapSpec {
            kind: MapKind::Inland,
            size,
            players,
        },
        ..SimConfig::default()
    }
}

fn flat(size: u16, players: u8) -> SimConfig {
    SimConfig {
        map: MapSpec {
            kind: MapKind::Flat,
            size,
            players,
        },
        ..SimConfig::default()
    }
}

/// The committed corpus.
///
/// Deliberately spans the *edges* of the config space as well as the middle.
/// The default config is the one everything is developed against, so it is the
/// one a bug is least likely to hide in.
pub fn corpus() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "battle-40v40",
            purpose: "eighty fighters: decisive combat without stranded units",
            seed: 1,
            ticks: simrunner::arena::BATTLE_LIMIT,
            config: simrunner::arena::config(),
            style: Style::Battle,
        },
        Scenario {
            name: "economy-2p",
            purpose: "the everyday case: two players gathering, building and training",
            seed: 1,
            ticks: 4_000,
            config: inland(128, 2),
            style: Style::Economy,
        },
        Scenario {
            name: "ages-2p",
            purpose: "rich enough to advance: research, age-ups, farms and the later buildings",
            seed: 12,
            ticks: 6_000,
            config: SimConfig {
                starting_stockpile: [5000; 4],
                ..inland(128, 2)
            },
            style: Style::Ages,
        },
        Scenario {
            name: "economy-8p",
            purpose: "every player slot in use, eight economies competing for room",
            seed: 20_260_905,
            ticks: 3_000,
            config: inland(168, 8),
            style: Style::Economy,
        },
        Scenario {
            name: "marching-crowd",
            purpose: "pathfinding and separation under load: everyone crossing at once",
            seed: 3,
            ticks: 3_000,
            config: inland(128, 4),
            style: Style::Marching,
        },
        Scenario {
            name: "idle-inland",
            purpose: "no commands at all, so map generation, animal wander and the \\n RNG are isolated from anything a player did",
            seed: 4,
            ticks: 2_000,
            config: inland(128, 4),
            style: Style::Idle,
        },
        Scenario {
            name: "smallest-map",
            purpose: "the minimum map size, where starts crowd and space runs out",
            seed: 5,
            ticks: 2_000,
            config: inland(48, 4),
            style: Style::Everything,
        },
        Scenario {
            name: "largest-map",
            purpose: "the maximum map size: long paths, sparse contact",
            seed: 6,
            ticks: 2_000,
            config: inland(256, 2),
            style: Style::Marching,
        },
        Scenario {
            name: "inland-no-wander",
            purpose: "wander off, so every RNG draw is attributable to a player's commands rather than to an animal",
            seed: 7,
            ticks: 2_500,
            config: SimConfig {
                wander: false,
                ..inland(96, 2)
            },
            style: Style::Economy,
        },
        Scenario {
            name: "flat-spawned-crowd",
            purpose: "a bare map with no start kit and no terrain: movement, separation and the entity store, isolated from mapgen",
            seed: 11,
            ticks: 2_500,
            config: SimConfig {
                wander: false,
                ..flat(96, 4)
            },
            style: Style::Marching,
        },
        Scenario {
            name: "entity-cap-pressed",
            purpose: "max_entities low enough to refuse spawns constantly, and a \\n tight population cap, so the refusal paths are the common case",
            seed: 8,
            ticks: 2_000,
            config: SimConfig {
                max_entities: 400,
                pop_cap_max: 12,
                ..inland(96, 4)
            },
            style: Style::Everything,
        },
        Scenario {
            name: "long-run",
            purpose: "20,000 ticks — a full 16-minute match at 20 Hz, so drift that \\n needs time to accumulate has time to accumulate",
            seed: 9,
            ticks: 20_000,
            config: inland(128, 2),
            style: Style::Economy,
        },
    ]
}

/// The scenarios `bench` measures. Sized against `docs/04` §12.
pub fn benchmarks() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "economy-2p",
            purpose: "a settled two-player economy",
            seed: 100,
            ticks: 2_000,
            config: inland(128, 2),
            style: Style::Economy,
        },
        Scenario {
            name: "marching-8p",
            purpose: "eight players pathing at once: the pathfinding load",
            seed: 101,
            ticks: 1_500,
            config: inland(168, 8),
            style: Style::Marching,
        },
        Scenario {
            name: "crowded",
            purpose: "the busiest thing the runner can produce",
            seed: 102,
            ticks: 1_500,
            config: inland(200, 8),
            style: Style::Everything,
        },
        Scenario {
            name: "battle-400",
            purpose: "four hundred soldiers fighting: the combat load",
            seed: 103,
            ticks: 1_500,
            config: SimConfig {
                wander: false,
                ..flat(96, 2)
            },
            style: Style::Melee,
        },
        Scenario {
            name: "opponents-8p",
            purpose: "eight Hard opponents on a full world: the AI load",
            seed: 104,
            ticks: 2_000,
            config: inland(168, 8),
            style: Style::Opponents,
        },
    ]
}

/// What [`Scenario::play`] tells about each tick when asked: the world
/// after it, its phases, and the opponents' thinking in nanoseconds.
pub type Observer<'a> = &'a mut dyn FnMut(&Simulation, Timings, u64);

/// Soldiers a side in a [`Style::Melee`] scenario.
const MELEE_SIDE: usize = 200;

/// What a melee side is made of, in spawn order: a mix, so the fight has
/// spears, archers and horses in it and not one kind walking into itself.
const MELEE_ROSTER: [KindId; 5] = [
    kinds::SPEARMAN,
    kinds::AXEMAN,
    kinds::BOWMAN,
    kinds::SLINGER,
    kinds::LIGHT_CAVALRY,
];

/// Units each opponent is handed at the start of a [`Style::Opponents`]
/// scenario, soldiers and villagers, so the world carries the §12 load
/// from the first tick instead of after twenty minutes of build-up.
const OPPONENT_KIT: usize = 40;

impl Scenario {
    /// Drives a live match and returns the replay of it.
    ///
    /// Command *choices* come from a separate RNG seeded off the match seed,
    /// so the stream is reproducible; command *targets* are read from the
    /// world, because a bot that cannot see the map cannot gather from it.
    /// The recorded replay is a flat command list either way, so verifying it
    /// is not circular.
    pub fn synthesise(&self) -> Replay {
        if self.style == Style::Battle {
            let battle = simrunner::arena::battle_40();
            assert!(battle.failure.is_none(), "{:?}", battle.failure);
            assert!(battle.winner().is_some(), "battle must finish decisively");
            return battle.replay;
        }
        let mut sim = Simulation::new(self.seed, self.config.clone());
        let mut bot = Rng::new(self.seed ^ 0xD1CE);
        let players = self.config.map.players.clamp(1, 8);
        for player in 0..players {
            self.deploy(&mut sim, &mut bot, player);
        }
        let mut opponents = self.opponents();

        while sim.tick() < self.ticks {
            if self.style != Style::Idle {
                for player in 0..players {
                    self.act(&mut sim, &mut bot, player);
                }
            }
            for bot in &mut opponents {
                let commands = {
                    let view = FoggedView::new(&sim, bot.player());
                    bot.think(&view)
                };
                for c in commands {
                    sim.issue_from(c, Source::Ai);
                }
            }
            sim.step();
        }
        sim.replay()
    }

    /// The computer opponents of a [`Style::Opponents`] scenario, Hard and
    /// one a side, seeded off the match; nobody for any other style.
    pub fn opponents(&self) -> Vec<Opponent> {
        if self.style != Style::Opponents {
            return Vec::new();
        }
        (0..self.config.map.players.clamp(1, 8))
            .map(|p| Opponent::new(p, Difficulty::Hard, self.seed))
            .collect()
    }

    /// One pass over the match as the benchmark measures it: the
    /// recording's commands replayed, except that a [`Style::Opponents`]
    /// scenario has its opponents think live in place of their recorded
    /// commands, because a recording holds what they said and not the time
    /// it took them to say it. Returns each tick's wall time in
    /// nanoseconds, the thinking included.
    ///
    /// With `observe`, the tick runs under [`Simulation::step_timed`] and
    /// each tick's phases and thinking time are reported; the tick times
    /// then carry the stopwatch's own cost, which is why the gate runs
    /// without it.
    pub fn play(
        &self,
        replay: &Replay,
        mut observe: Option<Observer<'_>>,
    ) -> Result<Vec<u128>, String> {
        replay.validate().map_err(|e| e.to_string())?;
        let mut opponents = self.opponents();
        let live = !opponents.is_empty();
        let mut sim = Simulation::new(replay.seed, replay.config.clone());
        let mut per_tick = Vec::with_capacity(replay.ticks as usize);
        let mut next = 0;
        let epoch = Instant::now();
        while sim.tick() < replay.ticks {
            let t0 = Instant::now();
            while let Some((tick, command)) = replay.commands.get(next) {
                if *tick != sim.tick() {
                    break;
                }
                let via = replay.sources.get(next).copied().unwrap_or_default();
                if !(live && via == Source::Ai) {
                    sim.issue_from(command.clone(), via);
                }
                next += 1;
            }
            let mut thinking = 0u64;
            for bot in &mut opponents {
                let t = Instant::now();
                let commands = {
                    let view = FoggedView::new(&sim, bot.player());
                    bot.think(&view)
                };
                thinking += t.elapsed().as_nanos() as u64;
                for c in commands {
                    sim.issue_from(c, Source::Ai);
                }
            }
            match observe.as_mut() {
                None => sim.step(),
                Some(o) => {
                    let mut clock = || epoch.elapsed().as_nanos() as u64;
                    let t = sim.step_timed(&mut clock);
                    o(&sim, t, thinking);
                }
            }
            per_tick.push(t0.elapsed().as_nanos());
        }
        if live && sim.replay().commands != replay.commands {
            return Err(format!(
                "{}: the opponents did not think what they thought when recorded",
                self.name
            ));
        }
        Ok(per_tick)
    }

    /// Tick 0 of a [`Style::Melee`] or [`Style::Opponents`] scenario: the
    /// crowds, spawned in one go so the load is there from the first tick.
    /// Nothing for any other style.
    fn deploy(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let spawn = |sim: &mut Simulation, kind: KindId, x: i32, y: i32| {
            sim.issue(Command {
                player,
                kind: CommandKind::Spawn {
                    kind,
                    pos: Vec2Fx::from_int(x, y),
                },
            });
        };
        match self.style {
            Style::Melee => {
                // Two blocks of ten ranks by twenty files facing each other
                // across the middle of the map.
                let (cx, cy) = melee_block(sim, player);
                for n in 0..MELEE_SIDE {
                    let kind = MELEE_ROSTER[n % MELEE_ROSTER.len()];
                    let rank = (n / 20) as i32;
                    let file = (n % 20) as i32;
                    let x = if player == 0 { cx - rank } else { cx + rank };
                    spawn(sim, kind, x, cy - 10 + file);
                }
            }
            Style::Opponents => {
                let Some(&(sx, sy)) = sim.starts().get(player as usize) else {
                    return;
                };
                let map = sim.map().width();
                for n in 0..OPPONENT_KIT {
                    // Three soldiers to a villager: an army the opponent's
                    // military manager will use, and workers its economy
                    // manager will place.
                    let kind = if n % 4 == 0 {
                        kinds::VILLAGER
                    } else {
                        MELEE_ROSTER[n % MELEE_ROSTER.len()]
                    };
                    let x = (sx + bot.range_i32(-6, 7)).clamp(0, map - 1);
                    let y = (sy + bot.range_i32(-6, 7)).clamp(0, map - 1);
                    spawn(sim, kind, x, y);
                }
            }
            _ => {}
        }
    }

    /// One player's turn to consider issuing something.
    fn act(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        // Roughly one decision per player every ten ticks: a busy human.
        if !bot.chance(1, 10) {
            return;
        }
        if self.style == Style::Ages {
            match bot.below(12) {
                0..=2 => self.gather(sim, bot, player),
                3..=6 => self.build(sim, bot, player),
                7 => self.train(sim, bot, player),
                8 => self.rally(sim, bot, player),
                9..=10 => self.research(sim, bot, player),
                _ => self.reseed(sim, bot, player),
            }
            return;
        }
        if self.style == Style::Melee {
            self.melee(sim, bot, player);
            return;
        }
        let economy = matches!(self.style, Style::Economy | Style::Everything);
        let marching = matches!(self.style, Style::Marching | Style::Everything);

        match bot.below(12) {
            0..=3 if economy => self.gather(sim, bot, player),
            4..=5 if economy => self.build(sim, bot, player),
            6 if economy => self.train(sim, bot, player),
            7 if economy => self.rally(sim, bot, player),
            8 if economy => self.research(sim, bot, player),
            9 if economy => self.reseed(sim, bot, player),
            10 if matches!(self.style, Style::Everything) => self.spawn(sim, bot, player),
            11 if marching => self.attack(sim, bot, player),
            _ if marching => self.march(sim, bot, player),
            // A style that has nothing to do this tick simply does nothing,
            // which keeps the decision RNG in step across styles.
            _ => {}
        }
    }

    /// Send idle villagers to the nearest node of a rotating resource.
    fn gather(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let idle = sim.idle_villagers(player);
        if idle.is_empty() {
            return;
        }
        let want: KindId = match bot.below(4) {
            0 => kinds::BERRY_BUSH,
            1 => kinds::TREE,
            2 => kinds::GOLD_MINE,
            _ => kinds::STONE_MINE,
        };
        let anchor = match sim.world().slot(idle[0]) {
            Some(s) => sim.world().pos[s.index()],
            None => return,
        };
        let node = nearest(sim, anchor, |k, r| k == want && r > 0);
        let Some(node) = node else { return };
        let take = 1 + bot.below(idle.len() as u32) as usize;
        sim.issue(Command {
            player,
            kind: CommandKind::Gather {
                ids: idle.into_iter().take(take).collect(),
                node,
            },
        });
    }

    /// Place a building on a legal tile near the player's start. Houses
    /// mostly; the rest of the roster now and then, which the simulation
    /// refuses until the age allows, so the refusal path is exercised too.
    fn build(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        if self.style == Style::Ages {
            return self.build_up(sim, bot, player);
        }
        let idle = sim.idle_villagers(player);
        if idle.is_empty() {
            return;
        }
        let kind = match bot.below(16) {
            0..=4 => kinds::HOUSE,
            5..=6 => kinds::STOREHOUSE,
            7 => kinds::BARRACKS,
            8 => kinds::FARM,
            9 => kinds::MARKET,
            10 => kinds::ARCHERY_RANGE,
            11 => kinds::WATCH_TOWER,
            12 => kinds::STONE_WALL,
            13 => kinds::GATE,
            _ => kinds::PALISADE_WALL,
        };
        let builders: Vec<EntityId> = idle.into_iter().take(3).collect();
        if kinds::is_wall(kind) {
            // A run of segments, as the player drags one: the builders go
            // to the first and carry on along it.
            return self.wall_run(sim, bot, player, kind, builders);
        }
        self.place(sim, bot, player, kind, builders);
    }

    /// A straight run of up to eight wall segments near the start, every
    /// tile the sim accepts (`UX-PLACE-03`).
    fn wall_run(
        &self,
        sim: &mut Simulation,
        bot: &mut Rng,
        player: PlayerId,
        kind: KindId,
        mut builders: Vec<EntityId>,
    ) {
        if builders.is_empty() {
            return;
        }
        let Some(&(sx, sy)) = sim.starts().get(player as usize) else {
            return;
        };
        let from = (sx + bot.range_i32(-12, 13), sy + bot.range_i32(-12, 13));
        let to = (from.0 + bot.range_i32(-8, 9), from.1 + bot.range_i32(-8, 9));
        for (x, y) in sim::nav::line_tiles(from, to) {
            if sim.can_place(player, kind, x, y).is_ok() {
                sim.issue(Command {
                    player,
                    kind: CommandKind::Build {
                        kind,
                        x,
                        y,
                        ids: std::mem::take(&mut builders),
                    },
                });
            }
        }
    }

    /// The ages bot's construction: finish the site in hand first, house
    /// when housed out, and otherwise put up the roster kind of the current
    /// age it has fewest of, three of each, so the gate for every age is
    /// met without the map filling with houses.
    fn build_up(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let world = sim.world();
        let site = world
            .slots()
            .find(|s| world.owner[s.index()] == player && world.construction[s.index()].is_some())
            .map(|s| world.id_at(s));
        if let Some(site) = site {
            let idle = sim.idle_villagers(player);
            if !idle.is_empty() {
                sim.issue(Command {
                    player,
                    kind: CommandKind::Assist { ids: idle, site },
                });
            }
            return;
        }
        let Some(pl) = sim.player(player) else {
            return;
        };
        let age = pl.age;
        let housed = pl.pop + 2 > pl.pop_cap;
        let fewest = kinds::all()
            .iter()
            .filter(|k| {
                k.buildable
                    && !k.mobile
                    && k.id != kinds::TOWN_CENTER
                    && k.id != kinds::HOUSE
                    && k.age <= age
            })
            .map(|k| (owned(sim, player, |o, _| o == k.id).len(), k.id))
            .min()
            .filter(|&(n, _)| n < 3)
            .map(|(_, id)| id);
        let kind = match (housed, fewest) {
            (true, _) => kinds::HOUSE,
            (false, Some(k)) => k,
            (false, None) => return,
        };
        let builders: Vec<EntityId> = owned(sim, player, |k, _| k == kinds::VILLAGER)
            .into_iter()
            .take(3)
            .collect();
        self.place(sim, bot, player, kind, builders);
    }

    /// Try a handful of nearby tiles and take the first the sim accepts, so
    /// the corpus records placements that actually happen.
    fn place(
        &self,
        sim: &mut Simulation,
        bot: &mut Rng,
        player: PlayerId,
        kind: KindId,
        builders: Vec<EntityId>,
    ) {
        if builders.is_empty() {
            return;
        }
        let Some(&(sx, sy)) = sim.starts().get(player as usize) else {
            return;
        };
        for _ in 0..8 {
            let x = sx + bot.range_i32(-10, 11);
            let y = sy + bot.range_i32(-10, 11);
            if sim.can_place(player, kind, x, y).is_ok() {
                sim.issue(Command {
                    player,
                    kind: CommandKind::Build {
                        kind,
                        x,
                        y,
                        ids: builders,
                    },
                });
                return;
            }
        }
    }

    /// Queue a technology — an age advance included — at a building that
    /// offers one. The simulation checks the gate; a refused command is a
    /// recorded command like any other.
    fn research(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let buildings = owned(sim, player, |k, _| {
            k == kinds::TOWN_CENTER || tech::at_building(k).next().is_some()
        });
        if buildings.is_empty() {
            return;
        }
        let building = buildings[bot.below(buildings.len() as u32) as usize];
        let Some(slot) = sim.world().slot(building) else {
            return;
        };
        let kind = sim.world().kind[slot.index()];
        let age = sim.player(player).map(|p| p.age);
        let mut techs: Vec<tech::TechId> = tech::at_building(kind).map(|t| t.id).collect();
        if kind == kinds::TOWN_CENTER {
            if let Some(t) = age.and_then(tech::age_advance) {
                techs.push(t.id);
            }
        }
        if techs.is_empty() {
            return;
        }
        let tech = techs[bot.below(techs.len() as u32) as usize];
        sim.issue(Command {
            player,
            kind: CommandKind::Research { building, tech },
        });
    }

    /// Flip farm auto-reseed now and then.
    fn reseed(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        if !bot.chance(1, 4) {
            return;
        }
        sim.issue(Command {
            player,
            kind: CommandKind::SetAutoReseed {
                enabled: bot.chance(3, 4),
            },
        });
    }

    /// Queue a unit at one of the training buildings: whatever the
    /// building offers, chosen at random, so soldiers come out of the
    /// Barracks, Range and Stable as well as villagers from the Town
    /// Center. The simulation refuses what the age does not allow yet.
    fn train(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let trainers = owned(sim, player, |k, _| kinds::info(k).trains);
        if trainers.is_empty() {
            return;
        }
        let building = trainers[bot.below(trainers.len() as u32) as usize];
        let Some(slot) = sim.world().slot(building) else {
            return;
        };
        let roster = sim.roster(player, sim.world().kind[slot.index()]);
        if roster.is_empty() {
            return;
        }
        let kind = roster[bot.below(roster.len() as u32) as usize];
        sim.issue(Command {
            player,
            kind: CommandKind::Train { building, kind },
        });
    }

    /// Point a training building's rally somewhere, sometimes at a node.
    fn rally(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let Some(building) = owned(sim, player, |k, _| kinds::info(k).trains)
            .into_iter()
            .next()
        else {
            return;
        };
        let map = sim.map().width();
        let rally = if bot.chance(1, 2) {
            let anchor = Vec2Fx::from_int(bot.range_i32(0, map), bot.range_i32(0, map));
            match nearest(sim, anchor, |k, r| k == kinds::TREE && r > 0) {
                Some(node) => Rally::Entity(node),
                None => Rally::None,
            }
        } else {
            Rally::Point(Vec2Fx::from_int(
                bot.range_i32(0, map),
                bot.range_i32(0, map),
            ))
        };
        sim.issue(Command {
            player,
            kind: CommandKind::SetRally { building, rally },
        });
    }

    /// Send a group on a long crossing.
    ///
    /// In a style that also runs an economy, working villagers are left
    /// alone. Marching everything drags them off the gold mid-trip, and since
    /// a march lands roughly as often as a gather, the economy never
    /// completes a single delivery — the first version of this scenario ran
    /// 2,000 ticks and gathered exactly nothing. A player moving their army
    /// does not move their villagers, so this is the realistic behaviour too.
    fn march(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let economy = matches!(self.style, Style::Economy | Style::Everything);
        let mut mobile = owned(sim, player, |k, _| {
            kinds::info(k).mobile && !(economy && k == kinds::VILLAGER)
        });
        if economy && mobile.is_empty() {
            // Nothing but villagers to send: take only the idle ones.
            mobile = sim.idle_villagers(player);
        }
        // A marching scenario exists to load pathfinding and separation, and
        // a start kit is five units — not a crowd. Top up towards a real one,
        // on a bare `Flat` map (which provides nothing at all) and on an
        // Inland map alike.
        if matches!(self.style, Style::Marching) && mobile.len() < CROWD {
            self.spawn(sim, bot, player);
            if mobile.is_empty() {
                return;
            }
        } else if mobile.is_empty() {
            return;
        }
        let map = sim.map().width();
        let n = 1 + bot.below(mobile.len() as u32) as usize;
        let start = bot.below(mobile.len() as u32) as usize;
        let ids: Vec<EntityId> = mobile.iter().cycle().skip(start).take(n).copied().collect();
        // Deliberately off-map sometimes; the sim must clamp rather than trust.
        let target = Vec2Fx::from_int(bot.range_i32(-8, map + 8), bot.range_i32(-8, map + 8));
        sim.issue(Command {
            player,
            kind: if bot.chance(1, 12) {
                CommandKind::Stop { ids }
            } else {
                CommandKind::Move { ids, target }
            },
        });
    }

    /// Send some soldiers at the enemy: an attack on something of theirs,
    /// or an attack-move on their start, with a stance or formation change
    /// now and then so those commands are in the corpus too.
    fn attack(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let fighters = owned(sim, player, |k, _| {
            let info = kinds::info(k);
            info.mobile && info.combat.attack > 0 && k != kinds::VILLAGER
        });
        if fighters.is_empty() {
            return;
        }
        let n = 1 + bot.below(fighters.len() as u32) as usize;
        let start = bot.below(fighters.len() as u32) as usize;
        let ids: Vec<EntityId> = fighters
            .iter()
            .cycle()
            .skip(start)
            .take(n)
            .copied()
            .collect();
        let players = sim.players().len() as u8;
        let enemy = (player + 1 + bot.below(players.max(2) as u32 - 1) as u8) % players.max(2);
        let kind = match bot.below(8) {
            0 => CommandKind::SetStance {
                ids,
                stance: sim::Stance::ALL[bot.below(4) as usize],
            },
            1 => CommandKind::SetFormation {
                ids,
                formation: sim::Formation::ALL[bot.below(5) as usize],
            },
            6 => {
                // Into the nearest tower or Town Center of ours.
                let shelters = owned(sim, player, |k, _| kinds::garrisons(k));
                if shelters.is_empty() {
                    return;
                }
                CommandKind::Garrison {
                    ids,
                    building: shelters[bot.below(shelters.len() as u32) as usize],
                }
            }
            7 => {
                let shelters = owned(sim, player, |k, _| kinds::garrisons(k));
                if shelters.is_empty() {
                    return;
                }
                CommandKind::Ungarrison {
                    building: shelters[bot.below(shelters.len() as u32) as usize],
                }
            }
            2 | 3 => {
                let theirs = owned(sim, enemy, |k, _| {
                    kinds::info(k).class != kinds::Class::Other
                });
                if theirs.is_empty() {
                    return;
                }
                CommandKind::Attack {
                    ids,
                    target: theirs[bot.below(theirs.len() as u32) as usize],
                }
            }
            _ => {
                let (x, y) = sim.starts().get(enemy as usize).copied().unwrap_or((0, 0));
                let target = Vec2Fx::from_int(x + bot.range_i32(-6, 7), y + bot.range_i32(-6, 7));
                if bot.chance(1, 3) {
                    CommandKind::Patrol { ids, target }
                } else {
                    CommandKind::AttackMove { ids, target }
                }
            }
        };
        sim.issue(Command { player, kind });
    }

    /// The fight of a [`Style::Melee`] scenario: some of a side's soldiers
    /// sent at the other side's block, most by attack-move, some at one
    /// enemy in particular, with a stance change now and then so it is
    /// not one formation walking into another.
    fn melee(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let fighters = owned(sim, player, |k, _| {
            let info = kinds::info(k);
            info.mobile && info.combat.attack > 0 && k != kinds::VILLAGER
        });
        if fighters.is_empty() {
            return;
        }
        let players = sim.players().len().max(2) as u8;
        let enemy = (player + 1) % players;
        let n = 1 + bot.below(fighters.len() as u32) as usize;
        let start = bot.below(fighters.len() as u32) as usize;
        let ids: Vec<EntityId> = fighters
            .iter()
            .cycle()
            .skip(start)
            .take(n)
            .copied()
            .collect();
        let kind = match bot.below(8) {
            0 => {
                let theirs = owned(sim, enemy, |k, _| kinds::info(k).mobile);
                if theirs.is_empty() {
                    return;
                }
                CommandKind::Attack {
                    ids,
                    target: theirs[bot.below(theirs.len() as u32) as usize],
                }
            }
            1 => CommandKind::SetStance {
                ids,
                stance: sim::Stance::ALL[bot.below(4) as usize],
            },
            _ => {
                let (cx, cy) = melee_block(sim, enemy);
                CommandKind::AttackMove {
                    ids,
                    target: Vec2Fx::from_int(cx + bot.range_i32(-8, 9), cy + bot.range_i32(-8, 9)),
                }
            }
        };
        sim.issue(Command { player, kind });
    }

    /// Free units, to press the entity cap and the population recount.
    ///
    /// Spawns a batch rather than one at a time: a marching scenario that
    /// seeds itself one unit per call ends up with a "crowd" of one per
    /// player, which exercises none of the separation or pathing load it
    /// exists to produce.
    fn spawn(&self, sim: &mut Simulation, bot: &mut Rng, player: PlayerId) {
        let map = sim.map().width();
        let batch = 1 + bot.below(12);
        for _ in 0..batch {
            sim.issue(Command {
                player,
                kind: CommandKind::Spawn {
                    kind: if bot.chance(1, 4) {
                        kinds::SCOUT
                    } else {
                        kinds::VILLAGER
                    },
                    pos: Vec2Fx::from_int(bot.range_i32(0, map), bot.range_i32(0, map)),
                },
            });
        }
    }
}

/// Where a side's block stands in a [`Style::Melee`] scenario: a quarter
/// of the way in from its edge, halfway down.
fn melee_block(sim: &Simulation, player: PlayerId) -> (i32, i32) {
    let w = sim.map().width();
    let h = sim.map().height();
    let x = if player == 0 { w / 4 } else { w - w / 4 };
    (x, h / 2)
}

/// Mobile units per player a [`Style::Marching`] scenario keeps in the field.
/// Large enough that units queue, jostle and displace one another, which is
/// the behaviour the scenario is there to pin down.
const CROWD: usize = 40;

/// Live entities of `player` matching `want`, in slot order.
fn owned(sim: &Simulation, player: PlayerId, want: impl Fn(KindId, i32) -> bool) -> Vec<EntityId> {
    sim.world()
        .slots()
        .filter(|s| {
            let i = s.index();
            sim.world().owner[i] == player && want(sim.world().kind[i], sim.world().resource[i])
        })
        .map(|s| sim.world().id_at(s))
        .collect()
}

/// The matching entity closest to `from`, by squared distance then slot order
/// so ties break deterministically.
fn nearest(sim: &Simulation, from: Vec2Fx, want: impl Fn(KindId, i32) -> bool) -> Option<EntityId> {
    let world = sim.world();
    world
        .slots()
        .filter(|s| want(world.kind[s.index()], world.resource[s.index()]))
        .min_by_key(|s| (from.distance_sq_raw(world.pos[s.index()]), s.index()))
        .map(|s| world.id_at(s))
}
