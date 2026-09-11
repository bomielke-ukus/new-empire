//! App input regressions: drive the same handlers as the window, then
//! inspect camera/selection or advance the real command queue. No GPU or
//! event loop is created; the live macOS smoke pass is still separate.

use super::*;
use sim::{Item, MapKind, MapSpec, Order};

fn app() -> App {
    let mut app = App::new();
    app.sim = Simulation::new(
        1,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Flat,
                size: 48,
                players: 2,
            },
            starting_stockpile: [5000; 4],
            wander: false,
            ..SimConfig::default()
        },
    );
    app.camera = Camera::new(48, 48, (1280.0, 720.0));
    app.clock.set_paused(true);
    app.input.edge_scroll = false;
    app
}

fn step(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        app.sim.step();
    }
    app.sim.check().unwrap();
}

fn spawn(app: &mut App, kind: u16, x: i32, y: i32) -> EntityId {
    app.issue(CommandKind::Spawn {
        kind,
        pos: Vec2Fx::from_int(x, y),
    });
    step(app, 3);
    app.sim
        .world()
        .slots()
        .filter(|s| app.sim.world().kind[s.index()] == kind)
        .map(|s| app.sim.world().id_at(s))
        .last()
        .unwrap()
}

fn draw(app: &mut App) {
    app.prev_pos.clone_from(&app.sim.world().pos);
    app.frame();
}

fn button(app: &App, label: &str) -> view::hud::Button {
    let b = app.hud.buttons.iter().find(|b| b.label == label).unwrap();
    assert!(b.enabled, "{label}: {}", b.reason);
    b.clone()
}

fn click(app: &mut App, b: &view::hud::Button) {
    app.left_press(b.x + b.w * 0.5, b.y + b.h * 0.5);
}

fn map_pixel(app: &App, u: f32, v: f32) -> (f32, f32) {
    let mm = app.minimap_rect();
    (
        mm.cx + (u - v) * mm.w * 0.5,
        mm.cy + (u + v - 1.0) * mm.h * 0.5,
    )
}

#[test]
fn minimap_clicks_and_scrubbing_take_priority_over_hud_and_placement() {
    let mut app = app();
    let unit = spawn(&mut app, kinds::VILLAGER, 8, 8);
    app.selection.set(vec![unit]);
    for viewport in [(1280.0, 720.0), (800.0, 600.0), (2560.0, 1600.0)] {
        app.camera.viewport = viewport;
        for (u, v) in [
            (0.0625, 0.0625),
            (0.5, 0.5),
            (0.9375, 0.9375),
            (0.875, 0.125),
        ] {
            app.camera.look_at_tile(2.0, 2.0);
            app.build_mode = Some(kinds::HOUSE);
            let (px, py) = map_pixel(&app, u, v);
            app.input.cursor = Some((px, py));
            app.left_press(px, py);
            let mut expected = app.camera;
            expected.look_at_tile(u * 48.0, v * 48.0);
            assert_eq!(app.camera.focus, expected.focus);
            assert!(app.input.scrubbing);
            assert!(app.selection.drag_from.is_none());
            assert_eq!(app.build_mode, Some(kinds::HOUSE));
            app.left_release(px, py);
            assert!(!app.input.scrubbing);
            assert_eq!(app.selection.ids, vec![unit]);
        }
    }
    let mm = app.minimap_rect();
    app.left_press(mm.cx, mm.cy);
    let (px, py) = map_pixel(&app, 0.25, 0.75);
    app.input
        .cursor_moved(&mut app.camera, mm, (48, 48), px, py);
    let mut expected = app.camera;
    expected.look_at_tile(12.0, 36.0);
    assert_eq!(app.camera.focus, expected.focus);
    app.left_release(px, py);
    assert_eq!(app.selection.ids, vec![unit]);
    step(&mut app, 3);
    assert_eq!(
        app.sim.world().len(),
        1,
        "minimap navigation must not place a house"
    );
}

#[test]
fn minimap_right_click_issues_moves_and_town_center_rallies() {
    let mut app = app();
    let unit = spawn(&mut app, kinds::VILLAGER, 8, 8);
    let tc = spawn(&mut app, kinds::TOWN_CENTER, 16, 16);
    app.selection.set(vec![unit]);
    let (px, py) = map_pixel(&app, 0.75, 0.75);
    app.right_press(px, py);
    step(&mut app, 3);
    assert_eq!(
        app.sim.world().order[unit.index()],
        Order::Move {
            target: Vec2Fx::from_int(36, 36)
        }
    );
    app.selection.set(vec![tc]);
    let (px, py) = map_pixel(&app, 0.5, 0.5);
    app.right_press(px, py);
    step(&mut app, 3);
    assert_eq!(
        app.sim.world().production[tc.index()]
            .as_ref()
            .unwrap()
            .rally,
        Some(Rally::Point(Vec2Fx::from_int(24, 24)))
    );
    app.build_mode = Some(kinds::HOUSE);
    app.right_press(px, py);
    assert_eq!(app.build_mode, None, "right-click still cancels placement");
}

#[test]
fn hud_outside_the_minimap_diamond_does_not_issue_world_commands() {
    let mut app = app();
    let unit = spawn(&mut app, kinds::VILLAGER, 8, 8);
    app.selection.set(vec![unit]);
    let mm = app.minimap_rect();
    let px = mm.cx + mm.w * 0.5;
    let py = mm.cy + mm.h * 0.5;
    assert!(mm.to_uv(px, py).is_none());
    let before = app.camera.focus;
    app.left_press(px, py);
    app.left_release(px, py);
    app.right_press(px, py);
    step(&mut app, 3);
    assert_eq!(app.camera.focus, before);
    assert_eq!(app.selection.ids, vec![unit]);
    assert_eq!(app.sim.world().order[unit.index()], Order::Idle);
}

fn research_settlement(app: &mut App) -> (EntityId, EntityId, EntityId) {
    // The Storehouse has a lower ID than the TC, so mixed selection shows
    // its queue while a search for a trainer would mistakenly return the TC.
    let store = spawn(app, kinds::STOREHOUSE, 10, 10);
    let tc = spawn(app, kinds::TOWN_CENTER, 16, 16);
    spawn(app, kinds::BARRACKS, 22, 22);
    let market = spawn(app, kinds::MARKET, 28, 28);
    app.issue(CommandKind::Research {
        building: tc,
        tech: tech::AGE_TOOL,
    });
    step(app, tech::info(tech::AGE_TOOL).unwrap().ticks() + 3);
    assert_eq!(app.sim.player(ME).unwrap().age, Age::Tool);
    (store, tc, market)
}

#[test]
fn storehouse_and_market_unqueue_refund_research_from_click_and_hotkey() {
    let mut app = app();
    let (store, _, market) = research_settlement(&mut app);
    for (building, technology) in [(store, tech::WOODWORKING), (market, tech::DOMESTICATION)] {
        for use_hotkey in [false, true] {
            app.selection.set(vec![building]);
            draw(&mut app);
            let before = app.sim.player(ME).unwrap().stockpile;
            let research = app
                .hud
                .buttons
                .iter()
                .find(|b| b.action == Action::Research(technology))
                .unwrap()
                .clone();
            assert!(research.enabled);
            click(&mut app, &research);
            step(&mut app, 3);
            assert!(app.sim.tech_queued(ME, technology));
            let cost = tech::info(technology).unwrap().cost;
            assert_eq!(
                app.sim.player(ME).unwrap().stockpile,
                std::array::from_fn(|i| before[i] - cost[i])
            );
            draw(&mut app);
            let cancel = button(&app, "UNQUEUE");
            if use_hotkey {
                assert!(app.hotkey(cancel.hotkey));
            } else {
                click(&mut app, &cancel);
            }
            step(&mut app, 3);
            assert!(!app.sim.tech_queued(ME, technology));
            assert!(!app.sim.player(ME).unwrap().has_researched(technology));
            assert_eq!(app.sim.player(ME).unwrap().stockpile, before);
            draw(&mut app);
            assert!(!app.hud.buttons.iter().any(|b| b.label == "UNQUEUE"));
        }
    }
}

#[test]
fn mixed_selection_unqueues_the_displayed_building_not_another_trainer() {
    let mut app = app();
    let (store, tc, _) = research_settlement(&mut app);
    app.issue(CommandKind::Research {
        building: store,
        tech: tech::WOODWORKING,
    });
    app.issue(CommandKind::Train {
        building: tc,
        kind: kinds::VILLAGER,
    });
    step(&mut app, 3);
    let before = app.sim.player(ME).unwrap().stockpile;
    app.selection.set(vec![store, tc]);
    draw(&mut app);
    let cancel = button(&app, "UNQUEUE");
    click(&mut app, &cancel);
    step(&mut app, 3);
    assert!(!app.sim.tech_queued(ME, tech::WOODWORKING));
    let q = app.sim.world().production[tc.index()].as_ref().unwrap();
    assert_eq!(q.queue.len(), 1);
    assert_eq!(q.queue[0].item, Item::Unit(kinds::VILLAGER));
    let refund = tech::info(tech::WOODWORKING).unwrap().cost;
    assert_eq!(
        app.sim.player(ME).unwrap().stockpile,
        std::array::from_fn(|i| before[i] + refund[i])
    );
    // The TC's own cancel path continues to work.
    app.selection.set(vec![tc]);
    draw(&mut app);
    assert!(app.hotkey('X'));
    step(&mut app, 3);
    assert!(app.sim.world().production[tc.index()]
        .as_ref()
        .unwrap()
        .queue
        .is_empty());
}
