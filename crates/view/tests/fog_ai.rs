//! M5 chunk 1: filtered observations and normal-command integration. The full
//! TA-AI-01 acceptance still needs human rendering/selection to consume fog.
use ai::ScoutAi;
use ai_api::{EntityKey, Intent, Tile};
use sim::{kinds, nav, Command, CommandKind, EntityId, MapKind, MapSpec, SimConfig, Simulation};
use view::fog::{command, Fog};

fn flat() -> Simulation {
    Simulation::new(
        7,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Flat,
                size: 64,
                players: 2,
            },
            wander: false,
            ..SimConfig::default()
        },
    )
}
fn run(sim: &mut Simulation, ticks: usize) {
    for _ in 0..ticks {
        sim.step();
    }
}
fn spawn(sim: &mut Simulation, player: u8, kind: u16, at: (i32, i32)) -> EntityId {
    sim.issue(Command {
        player,
        kind: CommandKind::Spawn {
            kind,
            pos: nav::centre(at),
        },
    });
    run(sim, 3);
    sim.world()
        .slots()
        .find(|s| {
            sim.world().owner[s.index()] == player
                && sim.world().kind[s.index()] == kind
                && nav::tile_of(sim.world().pos[s.index()]) == at
        })
        .map(|s| sim.world().id_at(s))
        .unwrap()
}
fn key(id: EntityId) -> EntityKey {
    EntityKey {
        index: id.index() as u32,
        generation: id.generation(),
    }
}
fn move_to(sim: &mut Simulation, player: u8, unit: EntityId, at: (i32, i32)) {
    sim.issue(
        command(
            sim,
            player,
            Intent::Move {
                unit: key(unit),
                target: Tile { x: at.0, y: at.1 },
            },
        )
        .unwrap(),
    );
}

#[test]
fn hidden_enemy_state_cannot_change_observations_or_decisions() {
    let mut a = flat();
    spawn(&mut a, 0, kinds::SCOUT, (10, 10));
    let enemy = spawn(&mut a, 1, kinds::VILLAGER, (50, 50));
    let mut b = a.clone();
    move_to(&mut b, 1, enemy, (52, 50));
    spawn(&mut b, 1, kinds::HOUSE, (45, 45));
    run(&mut a, 3);
    let mut fa = Fog::new(&a, 0).unwrap();
    let mut fb = Fog::new(&b, 0).unwrap();
    let mut aa = ScoutAi::default();
    let mut ab = ScoutAi::default();
    for _ in 0..100 {
        let ia = aa.decide(fa.update(&a));
        let ib = ab.decide(fb.update(&b));
        assert_eq!(fa.observation(), fb.observation());
        assert_eq!(ia, ib);
        a.step();
        b.step();
    }
    assert_ne!(a.state_hash(), b.state_hash());
    let hidden = fa.observation().view().tile(Tile { x: 50, y: 50 }).unwrap();
    assert!(!hidden.visible && hidden.terrain.is_none() && hidden.building.is_none());
    assert!(fa.observation().entities.iter().all(|e| e.owner == 0));
    assert!(Fog::new(&a, 2).is_none());
    assert!(Fog::new(&a, kinds::GAIA).is_none());
}

#[test]
fn revealed_enemy_has_no_order_or_live_state_after_leaving_vision() {
    let mut sim = flat();
    spawn(&mut sim, 0, kinds::VILLAGER, (10, 10));
    let enemy = spawn(&mut sim, 1, kinds::VILLAGER, (12, 10));
    let mut fog = Fog::new(&sim, 0).unwrap();
    let view = fog.update(&sim);
    assert_eq!(
        view.entities()
            .iter()
            .find(|e| e.id == key(enemy))
            .unwrap()
            .idle,
        None
    );
    move_to(&mut sim, 1, enemy, (24, 10));
    for _ in 0..200 {
        sim.step();
        fog.update(&sim);
    }
    assert!(fog
        .observation()
        .entities
        .iter()
        .all(|e| e.id != key(enemy)));
}

#[test]
fn moved_and_despawned_sources_leave_explored_tiles_and_stale_building_memory() {
    let mut sim = flat();
    let scout = spawn(&mut sim, 0, kinds::SCOUT, (10, 10));
    let house = spawn(&mut sim, 1, kinds::HOUSE, (10, 13));
    let mut fog = Fog::new(&sim, 0).unwrap();
    let tile = Tile { x: 10, y: 13 };
    assert_eq!(
        fog.update(&sim).tile(tile).unwrap().building.unwrap().id,
        key(house)
    );
    move_to(&mut sim, 0, scout, (35, 10));
    for _ in 0..160 {
        sim.step();
        fog.update(&sim);
    }
    assert!(!fog.observation().view().tile(tile).unwrap().visible);
    let remembered = *fog.observation().view().tile(tile).unwrap();
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Despawn { id: house },
    });
    for _ in 0..3 {
        sim.step();
        fog.update(&sim);
    }
    assert_eq!(*fog.observation().view().tile(tile).unwrap(), remembered);
    move_to(&mut sim, 0, scout, (10, 10));
    for _ in 0..160 {
        sim.step();
        fog.update(&sim);
    }
    let rediscovered = fog.observation().view().tile(tile).unwrap();
    assert!(rediscovered.visible && rediscovered.building.is_none());
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Despawn { id: scout },
    });
    for _ in 0..3 {
        sim.step();
        fog.update(&sim);
    }
    assert!(fog.observation().tiles.iter().all(|t| !t.visible));
    assert!(fog.observation().tiles.iter().any(|t| t.terrain.is_some()));
    // A new entity can reuse a slot without inheriting its old vision stamp.
    spawn(&mut sim, 0, kinds::SCOUT, (50, 50));
    fog.update(&sim);
    assert!(!fog.observation().view().tile(tile).unwrap().visible);
    assert!(
        fog.observation()
            .view()
            .tile(Tile { x: 50, y: 50 })
            .unwrap()
            .visible
    );
}

#[test]
fn adapter_rejects_foreign_stale_and_invalid_commands() {
    let mut sim = flat();
    let scout = spawn(&mut sim, 0, kinds::SCOUT, (10, 10));
    let house = spawn(&mut sim, 0, kinds::HOUSE, (20, 20));
    let intent = |id, x| Intent::Move {
        unit: key(id),
        target: Tile { x, y: 10 },
    };
    assert!(command(&sim, 1, intent(scout, 12)).is_none());
    assert!(command(&sim, 99, intent(scout, 12)).is_none());
    assert!(command(&sim, 0, intent(house, 12)).is_none());
    assert!(command(&sim, 0, intent(scout, -1)).is_none());
    assert!(command(&sim, 0, intent(scout, 64)).is_none());
    assert!(command(
        &sim,
        0,
        intent(
            EntityId::from_parts(scout.index() as u32, scout.generation() + 1),
            12
        )
    )
    .is_none());
    let before = sim.world().pos[scout.index()];
    let queued = command(&sim, 0, intent(scout, 14)).unwrap();
    let issue_tick = sim.tick();
    assert_eq!(sim.issue(queued.clone()), issue_tick + sim::COMMAND_DELAY);
    assert_eq!(sim.replay().commands.last(), Some(&(issue_tick, queued)));
    run(&mut sim, 2);
    assert_eq!(sim.world().pos[scout.index()], before);
    sim.step();
    assert_ne!(sim.world().pos[scout.index()], before);
}

#[test]
fn scouting_is_deterministic_obeys_cadence_and_replays_without_ai() {
    fn play() -> (Simulation, usize, usize) {
        let mut sim = flat();
        spawn(&mut sim, 0, kinds::SCOUT, (16, 16));
        let mut fog = Fog::new(&sim, 0).unwrap();
        fog.update(&sim);
        let initial = fog
            .observation()
            .tiles
            .iter()
            .filter(|t| t.terrain.is_some())
            .count();
        let mut ai = ScoutAi::default();
        let mut moves = 0;
        for _ in 0..500 {
            let view = fog.update(&sim);
            let intent = ai.decide(view);
            assert_eq!(ai.decide(view), None, "same tick must not issue twice");
            if let Some(intent) = intent {
                sim.issue(command(&sim, 0, intent).unwrap());
                moves += 1;
            }
            sim.step();
            sim.check().unwrap();
        }
        fog.update(&sim);
        assert!((2..=25).contains(&moves), "moves: {moves}");
        let explored = fog
            .observation()
            .tiles
            .iter()
            .filter(|t| t.terrain.is_some())
            .count();
        assert!(explored > initial, "the scout must discover terrain");
        (sim, moves, explored)
    }
    let (a, moves, explored) = play();
    let (b, _, _) = play();
    assert_eq!(a.replay(), b.replay());
    assert_eq!(a.state_hash(), b.state_hash());
    // Pinned on all CI platforms; a policy change must explain its new replay.
    assert_eq!(a.state_hash(), 0xe80d429eb73b3192);
    let replayed = a.replay().run(|_, _| {}).unwrap();
    assert_eq!(a.state_hash(), replayed.state_hash());
    println!(
        "500-tick scout: {moves} moves, {explored} explored tiles, hash {:016x}",
        a.state_hash()
    );
}

#[test]
fn busy_scouts_and_non_scouts_are_not_ordered() {
    let mut sim = flat();
    spawn(&mut sim, 0, kinds::VILLAGER, (10, 10));
    let mut fog = Fog::new(&sim, 0).unwrap();
    assert_eq!(ScoutAi::default().decide(fog.update(&sim)), None);
    let scout = spawn(&mut sim, 0, kinds::SCOUT, (16, 16));
    move_to(&mut sim, 0, scout, (24, 16));
    run(&mut sim, 3);
    assert_eq!(ScoutAi::default().decide(fog.update(&sim)), None);
}
