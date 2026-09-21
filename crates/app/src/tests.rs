//! App input regressions: drive the same handlers as the window, then
//! inspect camera/selection or advance the real command queue. No GPU or
//! event loop is created; the live macOS smoke pass is still separate.

use super::*;
use ai::Difficulty;
use audio::score::{BED_GAIN, COMBAT_IN_MS, CROSSFADE_MS};
use audio::{Bed, Fade, Layer, Play};
use sim::Task;
use sim::{Command, Formation, Item, MapKind, MapSpec, Order, SimConfig, Stance};
use view::shell::{Field, MapSize};
use view::{Control, NoticeKind, Settings, ShellButton};

#[test]
fn camera_keys_pan_without_building_or_spending_and_release_stops_panning() {
    let mut app = app();
    let (store, _, market) = research_settlement(&mut app);
    let unit = spawn(&mut app, kinds::VILLAGER, 8, 8);
    for selection in [vec![unit], vec![store], vec![market], vec![unit, store]] {
        app.selection.set(selection);
        let selection = app.selection.ids.clone();
        draw(&mut app);
        for code in [
            KeyCode::KeyW,
            KeyCode::KeyA,
            KeyCode::KeyS,
            KeyCode::KeyD,
            KeyCode::ArrowUp,
            KeyCode::ArrowLeft,
            KeyCode::ArrowDown,
            KeyCode::ArrowRight,
        ] {
            let commands = app.sim.replay().commands.len();
            let stockpile = app.sim.player(ME).unwrap().stockpile;
            app.camera.look_at_tile(24.0, 24.0);
            let before = app.camera.focus;
            assert!(!app.keyboard_input(code, ElementState::Pressed, false));
            app.input.update_camera(&mut app.camera, 0.1);
            assert_ne!(app.camera.focus, before);
            assert_eq!(app.build_mode, None);
            assert_eq!(app.sim.replay().commands.len(), commands);
            assert_eq!(app.selection.ids, selection);
            assert!(!app.keyboard_input(code, ElementState::Released, false));
            let stopped = app.camera.focus;
            app.input.update_camera(&mut app.camera, 0.1);
            assert_eq!(app.camera.focus, stopped);
            step(&mut app, 3);
            assert_eq!(app.sim.player(ME).unwrap().stockpile, stockpile);
        }
    }
}

#[test]
fn replacement_shortcuts_work_without_panning_or_key_repeat_orders() {
    let mut app = app();
    let (store, _, _) = research_settlement(&mut app);
    let unit = spawn(&mut app, kinds::VILLAGER, 8, 8);
    app.selection.set(vec![unit]);
    for (code, kind) in [
        (KeyCode::KeyO, kinds::STOREHOUSE),
        (KeyCode::KeyN, kinds::ARCHERY_RANGE),
    ] {
        draw(&mut app);
        let before = app.camera.focus;
        app.keyboard_input(code, ElementState::Pressed, false);
        app.input.update_camera(&mut app.camera, 0.1);
        assert_eq!(app.camera.focus, before);
        assert_eq!(app.build_mode, Some(kind));
        app.keyboard_input(code, ElementState::Released, false);
        assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
        assert_eq!(app.build_mode, None);
    }
    // J opens the defences page; J again on the page places the tower.
    draw(&mut app);
    let before = app.camera.focus;
    app.keyboard_input(KeyCode::KeyJ, ElementState::Pressed, false);
    app.input.update_camera(&mut app.camera, 0.1);
    assert_eq!(app.camera.focus, before);
    assert!(app.defences && app.build_mode.is_none());
    app.keyboard_input(KeyCode::KeyJ, ElementState::Released, false);
    draw(&mut app);
    app.keyboard_input(KeyCode::KeyJ, ElementState::Pressed, false);
    assert_eq!(app.build_mode, Some(kinds::WATCH_TOWER));
    assert!(!app.defences);
    app.keyboard_input(KeyCode::KeyJ, ElementState::Released, false);
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert_eq!(app.build_mode, None);
    app.selection.set(vec![store]);
    draw(&mut app);
    let before = app.camera.focus;
    let commands = app.sim.replay().commands.len();
    app.keyboard_input(KeyCode::KeyE, ElementState::Pressed, false);
    app.keyboard_input(KeyCode::KeyE, ElementState::Pressed, true);
    app.input.update_camera(&mut app.camera, 0.1);
    app.keyboard_input(KeyCode::KeyE, ElementState::Released, false);
    assert_eq!(app.camera.focus, before);
    assert_eq!(app.sim.replay().commands.len(), commands + 1);
    step(&mut app, 3);
    assert!(app.sim.tech_queued(ME, tech::STONE_MINING));
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert!(app.selection.ids.is_empty());
    // With nothing left to cancel, Escape opens the pause menu rather
    // than closing the window.
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert!(app.menu);
}

#[test]
fn a_retina_display_scales_the_hud_and_the_wheel_steps_whole_levels() {
    let mut app = app();
    let v = spawn(&mut app, kinds::VILLAGER, 20, 20);
    app.selection.set(vec![v]);
    draw(&mut app);
    let plain: Vec<_> = app
        .hud
        .buttons
        .iter()
        .map(|b| (b.label.clone(), b.x, b.y, b.w, b.h))
        .collect();
    assert!(!plain.is_empty());

    // The same window on a 2x display: twice the device pixels.
    app.camera.viewport = (2560.0, 1440.0);
    app.camera.dpi = 2.0;
    draw(&mut app);
    for (a, b) in plain.iter().zip(&app.hud.buttons) {
        assert_eq!(a.0, b.label);
        assert_eq!(
            (b.x, b.y, b.w, b.h),
            (a.1 * 2.0, a.2 * 2.0, a.3 * 2.0, a.4 * 2.0)
        );
    }
    assert!(
        app.over_hud(100.0, 1440.0 - 10.0) && !app.over_hud(100.0, 1440.0 - 300.0),
        "the panel band is scaled too"
    );
    // Clicking a scaled button still works.
    let house = button(&app, "HOUSE");
    click(&mut app, &house);
    assert_eq!(app.build_mode, Some(kinds::HOUSE));
    app.build_mode = None;

    // Zoom is a level, not a device-pixel ratio: 1x on this display draws
    // two device pixels per sprite pixel.
    assert_eq!(app.camera.zoom_level(), 1.0);
    assert_eq!(app.camera.zoom(), 2.0);

    // Trackpad deltas accumulate; a notch steps at once; the cursor's
    // world point stays put.
    app.input.cursor = Some((900.0, 500.0));
    let under = app.camera.window_to_world(900.0, 500.0);
    for _ in 0..5 {
        app.wheel(None, Some(8.0));
    }
    assert_eq!(app.camera.zoom_level(), 1.0, "40 px is not yet a step");
    app.wheel(None, Some(25.0));
    assert_eq!(app.camera.zoom_level(), 1.5);
    let after = app.camera.window_to_world(900.0, 500.0);
    assert!((under.0 - after.0).abs() < 1e-2 && (under.1 - after.1).abs() < 1e-2);
    app.wheel(Some(-1.0), None);
    assert_eq!(app.camera.zoom_level(), 1.0);
    app.wheel(Some(-10.0), None);
    assert_eq!(
        app.camera.zoom_level(),
        0.5,
        "clamped at the overview level"
    );

    // F2 cycles the player's UI scale on top of the display scale.
    app.keyboard_input(KeyCode::F2, ElementState::Pressed, false);
    assert_eq!(app.ui_scale(), 3.0);
    app.keyboard_input(KeyCode::F2, ElementState::Pressed, false);
    assert_eq!(app.ui_scale(), 4.0);
    app.keyboard_input(KeyCode::F2, ElementState::Pressed, false);
    assert_eq!(app.ui_scale(), 2.0);
}

#[test]
fn f1_opens_the_controls_and_escape_closes_them_first() {
    let mut app = app();
    draw(&mut app);
    let plain = app.hud.sprites.len();
    assert!(!app.keyboard_input(KeyCode::F1, ElementState::Pressed, false));
    assert!(app.show_help);
    draw(&mut app);
    assert!(app.hud.sprites.len() > plain + 200, "the overlay is drawn");
    // Escape closes the overlay before it touches placement or selection.
    app.build_mode = Some(kinds::HOUSE);
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert!(!app.show_help);
    assert_eq!(app.build_mode, Some(kinds::HOUSE));
    // `?` is the other way in, and a key repeat does not flicker it.
    assert!(!app.keyboard_input(KeyCode::Slash, ElementState::Pressed, false));
    assert!(app.show_help);
    assert!(!app.keyboard_input(KeyCode::Slash, ElementState::Pressed, true));
    assert!(app.show_help);
}

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
    app.settings.edge_scroll = false;
    app.settings_path = scratch("helper-settings").join("settings.ron");
    app.apply_settings();
    // Every sound asked for is recorded, so a test can hear it.
    app.speaker = Speaker::Recorder(Default::default());
    // Straight into a match, as the shell would after START. The world is
    // empty until a test spawns into it, which the shell would call a
    // decided match; the results panel is put away so the world takes
    // input. Anything recorded or saved goes to a scratch directory.
    app.shell = Shell::Match;
    app.results = ResultsState::Dismissed;
    app.saves_dir = scratch("helper-saves");
    app.replays_dir = scratch("helper-replays");
    app
}

/// What has played since the last time this was asked.
fn plays(app: &mut App) -> Vec<Play> {
    app.speaker.take()
}

/// A scratch directory for this process, under the system's temp dir.
fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("new-empire-app-{name}-{}", std::process::id()))
}

/// Clicks the shell button for `action` on the screen as last drawn.
fn press(app: &mut App, action: ShellAction) {
    draw(app);
    let b = app
        .screen
        .buttons
        .iter()
        .find(|b| b.action == action)
        .cloned()
        .unwrap_or_else(|| panic!("no button for {action:?}"));
    assert!(b.enabled, "{action:?}: {}", b.reason);
    app.left_press(b.x + b.w * 0.5, b.y + b.h * 0.5);
    app.left_release(b.x + b.w * 0.5, b.y + b.h * 0.5);
}

/// The shell button for `action` on the screen as last drawn.
fn shell_button(app: &App, action: ShellAction) -> ShellButton {
    app.screen
        .buttons
        .iter()
        .find(|b| b.action == action)
        .cloned()
        .unwrap_or_else(|| panic!("no button for {action:?}"))
}

/// A villager for each of two sides, so both stand and nothing is
/// decided.
fn two_sides(app: &mut App) {
    spawn(app, kinds::VILLAGER, 8, 8);
    app.sim.issue(Command {
        player: 1,
        kind: CommandKind::Spawn {
            kind: kinds::VILLAGER,
            pos: Vec2Fx::from_int(40, 40),
        },
    });
    step(app, 3);
    assert!(app.sim.standing(0) && app.sim.standing(1));
    app.results = ResultsState::Pending;
    app.clock.set_paused(false);
}

/// The game opens on the title; a skirmish is set up with the screen's
/// buttons and started; the setup is the match, and the opponents play
/// from the first tick as the AI while the human's queue stays theirs.
///
/// REQ: GD-AI-01
#[test]
fn a_skirmish_is_set_up_on_the_screens_and_the_opponents_play_as_the_ai() {
    let mut app = App::new();
    app.camera.viewport = (1280.0, 720.0);
    app.input.edge_scroll = false;
    assert_eq!(app.shell, Shell::Title);
    draw(&mut app);
    assert!(app.hud.buttons.is_empty(), "no HUD on the title");
    assert_eq!(app.screen.buttons.len(), 5);
    // Letters and clicks on the title reach no match.
    let commands = app.sim.replay().commands.len();
    assert!(!app.keyboard_input(KeyCode::KeyV, ElementState::Pressed, false));
    app.left_press(640.0, 100.0);
    app.left_release(640.0, 100.0);
    app.right_press(640.0, 100.0);
    assert_eq!(app.sim.replay().commands.len(), commands);
    assert_eq!(app.shell, Shell::Title);
    // Enter opens the setup; the arrows add an opponent and make it
    // Hardest, and raise the population cap.
    assert!(!app.keyboard_input(KeyCode::Enter, ElementState::Pressed, false));
    assert_eq!(app.shell, Shell::Setup);
    app.setup.seed = 3;
    app.preview();
    press(&mut app, ShellAction::Adjust(Field::Opponents, 1));
    press(&mut app, ShellAction::Adjust(Field::Difficulty(1), -1));
    press(&mut app, ShellAction::Adjust(Field::Difficulty(1), -1));
    press(&mut app, ShellAction::Adjust(Field::PopCap, 1));
    assert_eq!(
        app.setup.opponents,
        vec![Difficulty::Standard, Difficulty::Hardest]
    );
    assert_eq!(app.setup.pop_cap, 100);
    assert_eq!(app.setup.seed, 3);
    assert_eq!(
        app.sim.config().map.players,
        3,
        "the preview follows the setup"
    );
    press(&mut app, ShellAction::Shuffle);
    app.setup.seed = 3;
    app.preview();
    // START: the setup is the match.
    press(&mut app, ShellAction::Start);
    assert_eq!(app.shell, Shell::Match);
    let config = app.sim.config().clone();
    assert_eq!(config.map.players, 3);
    assert_eq!(config.map.size, 128);
    assert_eq!(config.pop_cap_max, 100);
    assert_eq!(
        config.gather_bonus_pct,
        vec![0, 0, sim::HARDEST_GATHER_BONUS_PCT],
        "only the Hardest opponent has the declared bonus"
    );
    assert_eq!(app.sim.seed(), 3);
    assert_eq!(app.sim.tick(), 0);
    assert_eq!(app.opponents.len(), 2);
    assert_eq!(app.opponents[0].player(), 1);
    assert_eq!(app.opponents[1].difficulty(), Difficulty::Hardest);
    assert!(!app.clock.paused());
    assert_eq!(app.results, ResultsState::Pending);
    // The opponents issue as the AI within their first thoughts; nothing
    // is issued in the human's name.
    let now = Instant::now();
    for _ in 0..200 {
        app.tick_once(now);
    }
    let replay = app.sim.replay();
    assert!(!replay.commands.is_empty());
    assert!(replay.commands.iter().all(|(_, c)| c.player != ME));
    assert!(replay.sources.iter().all(|s| *s == Source::Ai));
    assert_eq!(replay.sources.len(), replay.commands.len());
    draw(&mut app);
    assert!(app.hud.sprites.len() > 40, "the HUD is back");
    assert!(
        app.screen.buttons.is_empty(),
        "no overlay over a live match"
    );
    assert!(!app.decided());
}

/// The setup screen runs the engine's check (`docs/04` §19): a refused
/// setup greys START and says why, and Enter will not start it.
#[test]
fn the_setup_screen_refuses_what_the_engine_refuses() {
    let mut app = App::new();
    app.camera.viewport = (1280.0, 720.0);
    app.shell = Shell::Setup;
    app.setup.pop_cap = 500;
    app.preview();
    assert!(app.setup_error.is_some());
    draw(&mut app);
    assert!(!shell_button(&app, ShellAction::Start).enabled);
    assert!(!app.keyboard_input(KeyCode::Enter, ElementState::Pressed, false));
    assert_eq!(app.shell, Shell::Setup);
    app.setup.pop_cap = 75;
    app.preview();
    assert!(app.setup_error.is_none());
    draw(&mut app);
    assert!(shell_button(&app, ShellAction::Start).enabled);
    assert!(!app.keyboard_input(KeyCode::Enter, ElementState::Pressed, false));
    assert_eq!(app.shell, Shell::Match);
    // Escape in the setup goes back; Escape on the title quits.
    app.quit_to_title();
    app.shell = Shell::Setup;
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert_eq!(app.shell, Shell::Title);
    assert!(app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert!(app.quit);
}

/// Escape opens the pause menu, which pauses and takes every key and
/// click; RESIGN takes two clicks and ends the match on the results
/// screen, which says why, and BACK TO TITLE leaves the match.
///
/// REQ: GD-WIN-01
#[test]
fn the_pause_menu_pauses_and_resigning_ends_the_match_on_the_results_screen() {
    let mut app = app();
    two_sides(&mut app);
    draw(&mut app);
    assert!(!app.decided());
    assert!(app.screen.buttons.is_empty());
    // Escape with nothing to cancel opens the menu and pauses.
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert!(app.menu && app.clock.paused());
    draw(&mut app);
    assert!(shell_button(&app, ShellAction::Resign).enabled);
    // The menu has the keys and the world: Space does not unpause, a
    // click on the world selects nothing, a hotkey trains nothing.
    let commands = app.sim.replay().commands.len();
    assert!(!app.keyboard_input(KeyCode::Space, ElementState::Pressed, false));
    assert!(app.clock.paused());
    let (px, py) = on_screen(&app, 8.5, 8.5, 0.0);
    app.left_press(px, py);
    app.left_release(px, py);
    assert!(app.selection.ids.is_empty());
    assert!(!app.keyboard_input(KeyCode::KeyV, ElementState::Pressed, false));
    assert_eq!(app.sim.replay().commands.len(), commands);
    // Resume restores the clock; a pause the player set stays set.
    press(&mut app, ShellAction::Resume);
    assert!(!app.menu && !app.clock.paused());
    app.clock.set_paused(true);
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    assert!(app.menu);
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    assert!(!app.menu && app.clock.paused(), "Escape resumes too");
    app.clock.set_paused(false);
    // Resign takes two clicks; the first arms the button.
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    press(&mut app, ShellAction::Resign);
    assert!(app.menu && app.confirm == Some(ShellAction::Resign));
    draw(&mut app);
    assert_eq!(
        shell_button(&app, ShellAction::Resign).label,
        "CONFIRM RESIGN"
    );
    assert_eq!(app.sim.replay().commands.len(), commands);
    press(&mut app, ShellAction::Resign);
    assert!(!app.menu);
    assert!(app
        .sim
        .replay()
        .commands
        .iter()
        .any(|(_, c)| c.player == ME && c.kind == CommandKind::Resign));
    step(&mut app, 3);
    draw(&mut app);
    assert!(!app.sim.standing(ME));
    assert_eq!(app.results, ResultsState::Shown);
    let r = app.results_now();
    assert_eq!(r.heading, "DEFEAT");
    assert_eq!(r.why, "YOU RESIGNED");
    assert_eq!(r.sides.len(), 2);
    assert_eq!(r.sides[0].name, "YOU");
    assert!(!r.sides[0].standing && r.sides[1].standing);
    // KEEP WATCHING puts the panel away and it stays away.
    press(&mut app, ShellAction::KeepWatching);
    assert_eq!(app.results, ResultsState::Dismissed);
    draw(&mut app);
    assert!(app.screen.buttons.is_empty());
    // The menu now greys RESIGN, and QUIT needs no second click.
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    draw(&mut app);
    assert!(!shell_button(&app, ShellAction::Resign).enabled);
    press(&mut app, ShellAction::QuitToTitle);
    assert_eq!(app.shell, Shell::Title);
    assert!(app.opponents.is_empty() && !app.menu);
}

/// A match saved from the pause menu or with F5 is listed on the title's
/// LOAD GAME screen, newest first, and resumes at its tick with its
/// opponents and its camera; a save from another build is refused on
/// that screen with both version numbers, and the game stays there.
///
/// REQ: TA-SAVE-01
/// REQ: TA-DET-06
#[test]
fn a_saved_match_is_listed_on_the_load_screen_and_resumes_where_it_was() {
    let dir = scratch("saves");
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::new();
    app.camera.viewport = (1280.0, 720.0);
    app.input.edge_scroll = false;
    app.saves_dir = dir.clone();
    app.replays_dir = scratch("saves-replays");
    app.setup.seed = 3;
    app.setup.opponents = vec![Difficulty::Hard];
    app.shell = Shell::Setup;
    app.preview();
    press(&mut app, ShellAction::Start);
    let now = Instant::now();
    for _ in 0..120 {
        app.tick_once(now);
    }
    app.camera.look_at_tile(30.0, 30.0);
    app.camera.set_zoom_index(1);
    let (focus, zoom) = (app.camera.focus, app.camera.zoom_index);
    let hash = app.sim.state_hash();
    // Save from the menu; the file is named for the moment and the tick.
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    press(&mut app, ShellAction::Save);
    assert!(app.saved_note().is_some_and(|n| n.starts_with("SAVED ")));
    let files = save::list(&dir);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].summary.tick, 120);
    assert_eq!(files[0].summary.players, 2);
    // F5 saves too, and the menu comes back with the note.
    press(&mut app, ShellAction::Resume);
    for _ in 0..40 {
        app.tick_once(now);
    }
    assert!(!app.keyboard_input(KeyCode::F5, ElementState::Pressed, false));
    assert_eq!(save::list(&dir).len(), 2);
    draw(&mut app);
    // Leave the match and load the first save from the title.
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    press(&mut app, ShellAction::QuitToTitle);
    press(&mut app, ShellAction::QuitToTitle);
    assert_eq!(app.shell, Shell::Title);
    press(&mut app, ShellAction::LoadGame);
    assert_eq!(app.shell, Shell::Load);
    assert_eq!(app.saves.len(), 2);
    assert_eq!(app.saves[0].summary.tick, 160, "newest first");
    press(&mut app, ShellAction::Load(1));
    assert_eq!(app.shell, Shell::Match);
    assert_eq!(app.sim.tick(), 120);
    assert_eq!(app.sim.state_hash(), hash);
    assert_eq!(app.sim.seed(), 3);
    assert_eq!(app.opponents.len(), 1);
    assert_eq!(app.opponents[0].difficulty(), Difficulty::Hard);
    assert_eq!(app.camera.focus, focus);
    assert_eq!(app.camera.zoom_index, zoom);
    assert_eq!(app.results, ResultsState::Pending);
    assert!(!app.menu && !app.clock.paused());
    assert!(app.age_up.is_none(), "no celebration for the age it was in");
    // It plays on, the opponent with it.
    let before = app.sim.replay().commands.len();
    for _ in 0..100 {
        app.tick_once(now);
    }
    assert!(app.sim.replay().commands.len() > before);
    assert_eq!(app.sim.tick(), 220);
    // A save from another build is refused with both numbers, on the
    // screen, and Enter (the newest) meets the same refusal.
    let text = std::fs::read_to_string(&files[0].path).unwrap();
    let old = text.replacen("state_version:1", "state_version:9", 1);
    std::fs::write(dir.join("20990101-000000-seed3-tick1-p2.ron"), old).unwrap();
    app.quit_to_title();
    press(&mut app, ShellAction::LoadGame);
    assert_eq!(app.saves.len(), 3);
    assert_eq!(app.saves[0].summary.saved_at, 4_070_908_800);
    press(&mut app, ShellAction::Load(0));
    assert_eq!(app.shell, Shell::Load);
    let err = app.load_error.clone().expect("the refusal is shown");
    assert!(
        err.contains("version 9") && err.contains("version 1"),
        "{err}"
    );
    let plain = {
        app.load_error = None;
        draw(&mut app);
        app.screen.sprites.len()
    };
    assert!(!app.keyboard_input(KeyCode::Enter, ElementState::Pressed, false));
    assert!(app.load_error.is_some());
    draw(&mut app);
    assert!(app.screen.sprites.len() > plain, "the error line is drawn");
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert_eq!(app.shell, Shell::Title);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every match played is recorded when it is decided or left, one file
/// per match named for when it started and how far it got; WATCH REPLAY
/// lists the recordings and plays one from tick 0 to its end, reaching
/// the recorded match's hash, through any side's eyes or nobody's, with
/// pause and speed, and nothing the watcher does issues a command.
///
/// REQ: TA-DET-05
#[test]
fn a_match_is_recorded_and_watched_back_to_the_same_hash() {
    let dir = scratch("replays");
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::new();
    app.camera.viewport = (1280.0, 720.0);
    app.input.edge_scroll = false;
    app.replays_dir = dir.clone();
    app.saves_dir = scratch("replays-saves");
    let _ = std::fs::remove_dir_all(&app.saves_dir);
    app.setup.seed = 5;
    app.setup.opponents = vec![Difficulty::Easy];
    app.shell = Shell::Setup;
    app.preview();
    press(&mut app, ShellAction::Start);
    // The ticks are compared below: no ticks from the wall clock.
    app.clock.set_paused(true);
    let now = Instant::now();
    for _ in 0..200 {
        app.tick_once(now);
    }
    assert!(save::replays::list(&dir).is_empty(), "nothing yet");
    // Resigning decides the match, and the match is recorded then.
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    press(&mut app, ShellAction::Resign);
    press(&mut app, ShellAction::Resign);
    for _ in 0..3 {
        app.tick_once(now);
    }
    draw(&mut app);
    assert_eq!(app.results, ResultsState::Shown);
    let recorded = save::replays::list(&dir);
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].summary.tick, 203);
    assert_eq!(recorded[0].summary.seed, 5);
    // Playing on and leaving replaces the recording, not adds to it.
    press(&mut app, ShellAction::KeepWatching);
    for _ in 0..10 {
        app.tick_once(now);
    }
    let final_hash = app.sim.state_hash();
    let commands = app.sim.replay().commands.len();
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    press(&mut app, ShellAction::QuitToTitle);
    assert_eq!(app.shell, Shell::Title);
    let recorded = save::replays::list(&dir);
    assert_eq!(recorded.len(), 1, "one recording per match");
    assert_eq!(recorded[0].summary.tick, 213);
    // WATCH REPLAY lists it; watching starts at tick 0 with no opponent
    // thinking, seen through player 1's eyes.
    press(&mut app, ShellAction::WatchReplay);
    assert_eq!(app.shell, Shell::Replays);
    assert_eq!(app.replays.len(), 1);
    press(&mut app, ShellAction::Watch(0));
    assert_eq!(app.shell, Shell::Match);
    app.clock.set_paused(true);
    assert!(app.playback.is_some());
    assert_eq!(app.sim.tick(), 0);
    assert_eq!(app.sim.seed(), 5);
    assert!(app.opponents.is_empty());
    assert_eq!(app.viewer, Some(ME));
    assert!(!app.playback_over());
    for _ in 0..50 {
        app.tick_once(now);
    }
    // Nothing the watcher does issues a command or saves.
    let issued = app.sim.replay().commands.len();
    draw(&mut app);
    assert!(!app.keyboard_input(KeyCode::KeyV, ElementState::Pressed, false));
    let (sx, sy) = app.sim.starts()[0];
    let (px, py) = on_screen(&app, sx as f32 + 0.5, sy as f32 + 0.5, 0.0);
    app.right_press(px, py);
    app.keyboard_input(KeyCode::Delete, ElementState::Pressed, false);
    app.keyboard_input(KeyCode::F5, ElementState::Pressed, false);
    app.issue(CommandKind::Resign);
    assert_eq!(app.sim.replay().commands.len(), issued);
    assert!(save::list(&app.saves_dir).is_empty());
    // Tab cycles the eyes: player 1, player 2, everyone's, player 1.
    app.keyboard_input(KeyCode::Tab, ElementState::Pressed, false);
    assert_eq!(app.viewer, Some(1));
    draw(&mut app);
    app.keyboard_input(KeyCode::Tab, ElementState::Pressed, false);
    assert_eq!(app.viewer, None);
    draw(&mut app);
    app.keyboard_input(KeyCode::Tab, ElementState::Pressed, false);
    assert_eq!(app.viewer, Some(0));
    // Speed goes to sixteen times in a replay; Space resumes and pauses.
    for _ in 0..5 {
        app.keyboard_input(KeyCode::BracketRight, ElementState::Pressed, false);
    }
    assert_eq!(app.clock.speed, 16.0);
    app.keyboard_input(KeyCode::Space, ElementState::Pressed, false);
    assert!(!app.clock.paused());
    app.keyboard_input(KeyCode::Space, ElementState::Pressed, false);
    assert!(app.clock.paused());
    // To the end: the world is the recorded match's, and the results say
    // where the recording ends.
    while !app.playback_over() {
        app.tick_once(now);
    }
    assert_eq!(app.sim.tick(), 213);
    assert_eq!(app.sim.state_hash(), final_hash);
    assert_eq!(app.sim.replay().commands.len(), commands);
    draw(&mut app);
    assert_eq!(app.results, ResultsState::Shown);
    let r = app.results_now();
    assert_eq!(r.heading, "REPLAY OVER");
    assert_eq!(r.sides[0].name, "PLAYER 1");
    assert_eq!(r.sides[1].name, "PLAYER 2");
    // The menu in a replay greys SAVE and RESIGN; QUIT needs no second
    // click and records nothing new.
    press(&mut app, ShellAction::KeepWatching);
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    draw(&mut app);
    assert!(!shell_button(&app, ShellAction::Save).enabled);
    assert!(!shell_button(&app, ShellAction::Resign).enabled);
    press(&mut app, ShellAction::QuitToTitle);
    assert_eq!(app.shell, Shell::Title);
    assert!(app.playback.is_none());
    assert_eq!(save::replays::list(&dir).len(), 1);
    // A recording from another build is refused on the screen.
    let text = std::fs::read_to_string(&recorded[0].path).unwrap();
    std::fs::write(
        dir.join("20990101-000000-seed5-tick9-p2.ron"),
        text.replacen("version:1", "version:7", 1),
    )
    .unwrap();
    press(&mut app, ShellAction::WatchReplay);
    assert_eq!(app.replays.len(), 2);
    press(&mut app, ShellAction::Watch(0));
    assert_eq!(app.shell, Shell::Replays);
    let err = app.load_error.clone().expect("the refusal is shown");
    assert!(err.contains("version 7"), "{err}");
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert_eq!(app.shell, Shell::Title);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&app.saves_dir);
}

/// The settings screen changes the HUD size, edge scrolling, the window
/// mode and a key, each change written to the file at once and read back
/// by a fresh app; a rebound key works in a match and the old one no
/// longer does; a refused key says why and changes nothing; Escape keeps
/// the old key; the overlay names the new one; DEFAULTS restores all.
///
/// REQ: GD-A11Y-02
#[test]
fn settings_are_edited_on_their_screen_kept_at_once_and_read_back() {
    let dir = scratch("settings");
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("settings.ron");
    let mut app = App::new();
    app.camera.viewport = (1280.0, 720.0);
    app.settings_path = path.clone();
    app.replays_dir = scratch("settings-replays");
    app.apply_settings();
    assert!(app.input.edge_scroll && app.ui_scale() == 1.0);
    press(&mut app, ShellAction::Settings);
    assert_eq!(app.shell, Shell::Settings);
    press(&mut app, ShellAction::SettingScale(1));
    assert_eq!(app.settings.ui_scale, 1.5);
    assert_eq!(app.ui_scale(), 1.5, "in effect at once");
    let text = std::fs::read_to_string(&path).expect("kept at once");
    assert!(text.contains("1.5"), "{text}");
    press(&mut app, ShellAction::ToggleEdgeScroll);
    assert!(!app.settings.edge_scroll && !app.input.edge_scroll);
    press(&mut app, ShellAction::ToggleFullscreen);
    assert!(app.settings.fullscreen);
    // Rebind PAUSE: CHANGE, then the key; the row says it is waiting.
    press(&mut app, ShellAction::Rebind(Control::Pause));
    assert_eq!(app.capturing, Some(Control::Pause));
    draw(&mut app);
    assert!(!shell_button(&app, ShellAction::Rebind(Control::Pause)).enabled);
    assert!(!app.keyboard_input(KeyCode::F6, ElementState::Pressed, false));
    assert_eq!(app.capturing, None);
    assert_eq!(app.settings.key(Control::Pause), "F6");
    assert!(std::fs::read_to_string(&path).unwrap().contains("F6"));
    // A command letter is refused with the reason, and nothing changes.
    press(&mut app, ShellAction::Rebind(Control::Faster));
    assert!(!app.keyboard_input(KeyCode::KeyH, ElementState::Pressed, false));
    assert_eq!(app.capturing, None);
    let err = app.settings_error.clone().expect("the refusal is shown");
    assert!(err.contains("COMMAND KEY"), "{err}");
    assert_eq!(app.settings.key(Control::Faster), "BracketRight");
    // Escape while waiting keeps the old key; Escape after goes back.
    press(&mut app, ShellAction::Rebind(Control::Faster));
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert_eq!(app.capturing, None);
    assert_eq!(app.shell, Shell::Settings);
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert_eq!(app.shell, Shell::Title);
    // A fresh app reads the file and applies it.
    let mut fresh = App::new();
    fresh.settings_path = path.clone();
    fresh.load_settings();
    assert_eq!(fresh.settings.ui_scale, 1.5);
    assert_eq!(fresh.ui_scale(), 1.5);
    assert!(!fresh.settings.edge_scroll && !fresh.input.edge_scroll);
    assert!(fresh.settings.fullscreen);
    assert_eq!(fresh.settings.key(Control::Pause), "F6");
    // In a match the new key pauses and the old one does nothing; the
    // overlay names the new one.
    app.setup.seed = 3;
    app.shell = Shell::Setup;
    app.preview();
    press(&mut app, ShellAction::Start);
    assert!(!app.clock.paused());
    app.keyboard_input(KeyCode::F6, ElementState::Pressed, false);
    assert!(app.clock.paused());
    app.keyboard_input(KeyCode::F6, ElementState::Pressed, false);
    assert!(!app.clock.paused());
    app.keyboard_input(KeyCode::Space, ElementState::Pressed, false);
    assert!(!app.clock.paused(), "Space is nobody's now");
    let [general, _] = view::hud::controls(&app.settings);
    assert!(general.contains(&("F6".to_string(), "PAUSE".to_string())));
    assert!(general.iter().any(|(k, _)| k == "WASD"));
    // A missing file is not an error; a broken one reports and defaults.
    let mut none = App::new();
    none.settings_path = dir.join("nothing.ron");
    none.load_settings();
    assert_eq!(none.settings, Settings::default());
    assert!(none.settings_error.is_none());
    std::fs::write(dir.join("broken.ron"), "(ui_scale: \"big\")").unwrap();
    let mut broken = App::new();
    broken.settings_path = dir.join("broken.ron");
    broken.load_settings();
    assert_eq!(broken.settings, Settings::default());
    assert!(broken.settings_error.is_some());
    // DEFAULTS restores everything and keeps it.
    app.quit_to_title();
    press(&mut app, ShellAction::Settings);
    press(&mut app, ShellAction::ResetSettings);
    assert_eq!(app.settings, Settings::default());
    assert_eq!(app.ui_scale(), 1.0);
    assert!(std::fs::read_to_string(&path).unwrap().contains("Space"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&app.replays_dir);
}

/// What happens to the side stacks up in the corner (`docs/03` §6.3):
/// an age reached, a technology researched, an attack with its place,
/// a loss; a notice with a place is clicked to put the camera there;
/// attacks in one area are one notice in twenty seconds.
///
/// REQ: UX-NOTIFY-01
#[test]
fn notices_stack_in_the_corner_and_a_click_puts_the_camera_there() {
    let mut app = app();
    let (store, _, _) = research_settlement(&mut app);
    draw(&mut app);
    assert!(app
        .notices
        .shown()
        .iter()
        .any(|n| n.kind == NoticeKind::Age && n.text == "TOOL AGE"));
    app.issue(CommandKind::Research {
        building: store,
        tech: tech::STONE_MINING,
    });
    step(
        &mut app,
        tech::info(tech::STONE_MINING).unwrap().ticks() + 3,
    );
    draw(&mut app);
    assert!(app
        .notices
        .shown()
        .iter()
        .any(|n| n.kind == NoticeKind::Research && n.text == "STONE MINING RESEARCHED"));
    // An enemy clubman on a villager of ours: the alarm, with its place.
    let villager = spawn(&mut app, kinds::VILLAGER, 30, 30);
    app.sim.issue(Command {
        player: 1,
        kind: CommandKind::Spawn {
            kind: kinds::CLUBMAN,
            pos: Vec2Fx::from_int(31, 30),
        },
    });
    step(&mut app, 3);
    let enemy = {
        let w = app.sim.world();
        w.slots()
            .find(|s| w.owner[s.index()] == 1 && w.kind[s.index()] == kinds::CLUBMAN)
            .map(|s| w.id_at(s))
            .unwrap()
    };
    app.sim.issue(Command {
        player: 1,
        kind: CommandKind::Attack {
            ids: vec![enemy],
            target: villager,
        },
    });
    let now = Instant::now();
    let mut ticks = 0;
    while ticks < 400
        && !app
            .notices
            .shown()
            .iter()
            .any(|n| n.kind == NoticeKind::Attack)
    {
        app.tick_once(now);
        ticks += 1;
    }
    let attack = app
        .notices
        .shown()
        .iter()
        .find(|n| n.kind == NoticeKind::Attack)
        .cloned()
        .expect("the alarm is on the stack");
    let (x, y) = attack.tile.expect("with its place");
    draw(&mut app);
    let jump = app
        .hud
        .buttons
        .iter()
        .find(|b| matches!(b.action, Action::Jump(_)) && b.label == "UNDER ATTACK")
        .cloned()
        .expect("a notice to click");
    assert!(jump.x < 20.0 && jump.y + jump.h <= 720.0 - BOTTOM_PANEL);
    app.camera.look_at_tile(x, y);
    let there = app.camera.focus;
    app.camera.look_at_tile(5.0, 5.0);
    assert_ne!(app.camera.focus, there);
    app.left_press(jump.x + 4.0, jump.y + 4.0);
    app.left_release(jump.x + 4.0, jump.y + 4.0);
    assert_eq!(app.camera.focus, there, "the click looks where it happened");
    assert!(app.selection.ids.is_empty(), "and selects nothing");
    // The villager dies: a loss. Attacks in one area stay one notice
    // per twenty seconds however many alarms the fight raises.
    let mut ticks = 0;
    while ticks < 2000
        && !app
            .notices
            .shown()
            .iter()
            .any(|n| n.kind == NoticeKind::Loss)
    {
        app.tick_once(now);
        ticks += 1;
    }
    assert!(app
        .notices
        .shown()
        .iter()
        .any(|n| n.kind == NoticeKind::Loss && n.text == "VILLAGER LOST"));
    let attacks: Vec<u64> = app
        .notices
        .shown()
        .iter()
        .filter(|n| n.kind == NoticeKind::Attack)
        .map(|n| n.tick)
        .collect();
    for w in attacks.windows(2) {
        assert!(w[1] - w[0] >= view::notify::ATTACK_TICKS, "{attacks:?}");
    }
}

/// The M6 acceptance (`docs/06`): from the title, a skirmish is set up
/// and played to the victory screen, saved in the middle, reloaded, and
/// watched back as a replay, every step through the screens' buttons and
/// the window's handlers and none through a terminal. The native half,
/// the same by hand on the Mac, is recorded in `docs/10` when it is done.
///
/// REQ: RM-M6-01
#[test]
fn a_full_skirmish_is_played_to_victory_saved_reloaded_and_watched_back() {
    let saves = scratch("acceptance-saves");
    let replays = scratch("acceptance-replays");
    let _ = std::fs::remove_dir_all(&saves);
    let _ = std::fs::remove_dir_all(&replays);
    let mut app = App::new();
    app.camera.viewport = (1280.0, 720.0);
    app.settings.edge_scroll = false;
    app.settings_path = scratch("acceptance-settings").join("settings.ron");
    app.apply_settings();
    app.saves_dir = saves.clone();
    app.replays_dir = replays.clone();
    assert_eq!(app.shell, Shell::Title);
    // NEW GAME: one Easy opponent on a Tiny map, START.
    press(&mut app, ShellAction::NewGame);
    app.setup.seed = 8;
    app.preview();
    press(&mut app, ShellAction::Adjust(Field::Size, -1));
    assert_eq!(app.setup.size, MapSize::Tiny);
    press(&mut app, ShellAction::Adjust(Field::Difficulty(0), -1));
    assert_eq!(app.setup.opponents, vec![Difficulty::Easy]);
    press(&mut app, ShellAction::Start);
    assert_eq!(app.shell, Shell::Match);
    // The test drives every tick itself: a redraw must not add ticks of
    // its own from the wall clock, since the ticks are compared below.
    app.clock.set_paused(true);
    // Half a minute in, a quick save.
    let now = Instant::now();
    for _ in 0..600 {
        app.tick_once(now);
    }
    assert!(!app.keyboard_input(KeyCode::F5, ElementState::Pressed, false));
    let saved_hash = app.sim.state_hash();
    assert_eq!(save::list(&saves).len(), 1);
    // The player's army, raised the way a test can, goes for the enemy
    // town and then for whatever of theirs still stands, as a player
    // would from the minimap, until the town is out.
    let (ex, ey) = app.sim.starts()[1];
    for i in 0..40 {
        app.issue(CommandKind::Spawn {
            kind: kinds::CLUBMAN,
            pos: Vec2Fx::from_int(ex + 8 + i % 8, ey + 8 + i / 8),
        });
    }
    for _ in 0..3 {
        app.tick_once(now);
    }
    let army: Vec<EntityId> = {
        let w = app.sim.world();
        w.slots()
            .filter(|s| w.owner[s.index()] == ME && w.kind[s.index()] == kinds::CLUBMAN)
            .map(|s| w.id_at(s))
            .collect()
    };
    assert_eq!(army.len(), 40);
    app.issue(CommandKind::SetStance {
        ids: army.clone(),
        stance: Stance::Aggressive,
    });
    let mut ticks = 0;
    while app.sim.winner().is_none() && ticks < 12_000 {
        if ticks % 100 == 0 {
            let target = {
                let w = app.sim.world();
                w.slots()
                    .filter(|s| {
                        w.owner[s.index()] == 1
                            && w.dying[s.index()] == 0
                            && w.health[s.index()] > sim::Fx::ZERO
                    })
                    .map(|s| w.pos[s.index()])
                    .next()
            };
            if let Some(target) = target {
                app.issue(CommandKind::AttackMove {
                    ids: army.clone(),
                    target,
                });
            }
        }
        app.tick_once(now);
        ticks += 1;
    }
    assert_eq!(
        app.sim.winner(),
        Some(ME),
        "the town is out within ten minutes"
    );
    draw(&mut app);
    assert_eq!(app.results, ResultsState::Shown);
    assert_eq!(app.results_now().heading, "VICTORY");
    let (final_hash, final_tick) = (app.sim.state_hash(), app.sim.tick());
    press(&mut app, ShellAction::QuitToTitle);
    assert_eq!(app.shell, Shell::Title);
    assert_eq!(save::replays::list(&replays).len(), 1);
    // LOAD GAME: the save resumes in the middle of the match.
    press(&mut app, ShellAction::LoadGame);
    press(&mut app, ShellAction::Load(0));
    assert_eq!(app.shell, Shell::Match);
    app.clock.set_paused(true);
    assert_eq!(app.sim.tick(), 600);
    assert_eq!(app.sim.state_hash(), saved_hash);
    assert_eq!(app.opponents.len(), 1);
    for _ in 0..20 {
        app.tick_once(now);
    }
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    press(&mut app, ShellAction::QuitToTitle);
    press(&mut app, ShellAction::QuitToTitle);
    assert_eq!(app.shell, Shell::Title);
    // WATCH REPLAY: the match that was won, back to its last tick.
    press(&mut app, ShellAction::WatchReplay);
    let row = app
        .replays
        .iter()
        .position(|e| e.summary.tick == final_tick)
        .expect("the won match is listed");
    press(&mut app, ShellAction::Watch(row));
    assert_eq!(app.shell, Shell::Match);
    assert!(app.playback.is_some());
    while !app.playback_over() {
        app.tick_once(now);
    }
    assert_eq!(app.sim.tick(), final_tick);
    assert_eq!(app.sim.state_hash(), final_hash);
    draw(&mut app);
    assert_eq!(app.results_now().heading, "REPLAY OVER");
    press(&mut app, ShellAction::QuitToTitle);
    assert_eq!(app.shell, Shell::Title);
    let _ = std::fs::remove_dir_all(&saves);
    let _ = std::fs::remove_dir_all(&replays);
}

/// The last side standing wins on the results screen; quitting a live
/// match to the title takes two clicks, and Escape disarms the first.
///
/// REQ: GD-WIN-01
#[test]
fn victory_shows_the_results_and_quitting_a_live_match_takes_two_clicks() {
    let mut app = app();
    two_sides(&mut app);
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    press(&mut app, ShellAction::QuitToTitle);
    assert!(app.menu && app.confirm == Some(ShellAction::QuitToTitle));
    assert_eq!(app.shell, Shell::Match);
    app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false);
    assert!(!app.menu && app.confirm.is_none());
    // The other side falls.
    let theirs: Vec<_> = {
        let w = app.sim.world();
        w.slots()
            .filter(|s| w.owner[s.index()] == 1)
            .map(|s| w.id_at(s))
            .collect()
    };
    for id in theirs {
        app.sim.issue(Command {
            player: 1,
            kind: CommandKind::Despawn { id },
        });
    }
    step(&mut app, 3);
    draw(&mut app);
    assert_eq!(app.sim.winner(), Some(ME));
    assert_eq!(app.results, ResultsState::Shown);
    let r = app.results_now();
    assert_eq!(r.heading, "VICTORY");
    assert_eq!(r.why, "EVERY OTHER SIDE IS OUT");
    assert!(r.sides[0].standing && !r.sides[1].standing);
    assert!(shell_button(&app, ShellAction::KeepWatching).enabled);
    press(&mut app, ShellAction::QuitToTitle);
    assert_eq!(
        app.shell,
        Shell::Title,
        "a decided match needs no second click"
    );
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

/// Soldiers' commands from the panel and the mouse: attack-move and
/// patrol pick a point, a right-click on an enemy attacks, stance and
/// formation buttons issue their commands, and a corpse is not selectable.
///
/// REQ: UX-CMD-02
/// REQ: UX-CMD-03
/// REQ: UX-CMD-07
#[test]
fn soldiers_attack_move_patrol_and_change_stance_from_the_panel() {
    let mut app = app();
    let club = spawn(&mut app, kinds::CLUBMAN, 10, 10);
    let bow = spawn(&mut app, kinds::BOWMAN, 11, 10);
    // An enemy, made passive so nothing happens until it is ordered.
    app.sim.issue(Command {
        player: 1,
        kind: CommandKind::Spawn {
            kind: kinds::CLUBMAN,
            pos: Vec2Fx::from_int(14, 10),
        },
    });
    step(&mut app, 3);
    let enemy = app
        .sim
        .world()
        .slots()
        .find(|s| app.sim.world().owner[s.index()] == 1)
        .map(|s| app.sim.world().id_at(s))
        .unwrap();
    app.sim.issue(Command {
        player: 1,
        kind: CommandKind::SetStance {
            ids: vec![enemy],
            stance: Stance::Passive,
        },
    });
    step(&mut app, 3);
    app.selection.set(vec![club, bow]);
    app.camera.look_at_tile(12.0, 10.0);
    draw(&mut app);

    // Attack-move: the button arms targeting, a click on the ground fires.
    let commands = app.sim.replay().commands.len();
    let attack_move = button(&app, "ATTACK MOVE");
    click(&mut app, &attack_move);
    assert_eq!(app.targeting, Some(Targeting::AttackMove));
    draw(&mut app);
    assert!(button(&app, "CANCEL").enabled, "the grid offers a way out");
    let (px, py) = app.camera.to_window(
        view::iso::project(20.0, 10.0, 0.0).0,
        view::iso::project(20.0, 10.0, 0.0).1,
    );
    app.left_press(px, py);
    assert_eq!(app.targeting, None);
    assert_eq!(app.sim.replay().commands.len(), commands + 1);
    step(&mut app, 3);
    assert!(matches!(
        app.sim.world().order[app.sim.world().slot(club).unwrap().index()],
        Order::AttackMove { .. }
    ));

    // Patrol by hotkey, cancelled by escape before any click.
    draw(&mut app);
    assert!(app.hotkey('P'));
    assert_eq!(app.targeting, Some(Targeting::Patrol));
    assert!(!app.keyboard_input(KeyCode::Escape, ElementState::Pressed, false));
    assert_eq!(app.targeting, None);

    // A right-click on the enemy attacks it.
    let commands = app.sim.replay().commands.len();
    draw(&mut app);
    let ep = app.sim.world().pos[app.sim.world().slot(enemy).unwrap().index()];
    let (ex, ey) = (view::fx_to_f32(ep.x), view::fx_to_f32(ep.y));
    let g = view::iso::project(ex, ey, view::iso::ground_height(app.sim.map(), ex, ey));
    let (px, py) = app.camera.to_window(g.0, g.1 - 8.0);
    app.right_press(px, py);
    assert_eq!(
        app.sim.replay().commands.len(),
        commands + 1,
        "an attack was issued"
    );
    step(&mut app, 3);
    assert!(matches!(
        app.sim.world().order[app.sim.world().slot(bow).unwrap().index()],
        Order::Attack { target, .. } if target == enemy
    ));

    // Stance and formation from the panel.
    draw(&mut app);
    let aggressive = button(&app, "AGGRESSIVE");
    click(&mut app, &aggressive);
    let formation = button(&app, "FORM: LINE");
    click(&mut app, &formation);
    step(&mut app, 3);
    let ci = app.sim.world().slot(club).unwrap().index();
    assert_eq!(app.sim.world().stance[ci], Stance::Aggressive);
    assert_eq!(
        app.sim.world().formation[ci],
        Formation::Box,
        "the next one round"
    );
    draw(&mut app);
    assert!(app.hud.buttons.iter().any(|b| b.label == "[AGGRESSIVE]"));

    // Kill the enemy: its corpse is not picked, so a click there deselects.
    for _ in 0..20 * 40 {
        app.sim.step();
        if app
            .sim
            .world()
            .slot(enemy)
            .is_none_or(|s| app.sim.world().dying[s.index()] > 0)
        {
            break;
        }
    }
    let es = app
        .sim
        .world()
        .slot(enemy)
        .expect("the corpse is still there");
    assert!(app.sim.world().dying[es.index()] > 0);
    draw(&mut app);
    let ep = app.sim.world().pos[es.index()];
    let (ex, ey) = (view::fx_to_f32(ep.x), view::fx_to_f32(ep.y));
    let g = view::iso::project(ex, ey, view::iso::ground_height(app.sim.map(), ex, ey));
    let (px, py) = app.camera.to_window(g.0, g.1 - 4.0);
    let picked = selection::pick(&app.scene, &app.atlas, &app.camera, &app.sim, px, py);
    assert_ne!(picked, Some(enemy), "a corpse is not a target");
}

/// The window position of a ground point, lifted `lift` pixels.
fn on_screen(app: &App, x: f32, y: f32, lift: f32) -> (f32, f32) {
    let g = view::iso::project(x, y, view::iso::ground_height(app.sim.map(), x, y));
    app.camera.to_window(g.0, g.1 - lift)
}

/// Garrison by right-click, out again from the panel, and a wall run
/// dragged from the defences page (`UX-CMD-09`, `UX-PLACE-03`).
///
/// REQ: UX-CMD-09
/// REQ: UX-PLACE-03
#[test]
fn units_garrison_on_a_right_click_and_walls_are_dragged_from_the_defences_page() {
    let mut app = app();
    let tc = spawn(&mut app, kinds::TOWN_CENTER, 12, 12);
    let a = spawn(&mut app, kinds::CLUBMAN, 16, 12);
    let b = spawn(&mut app, kinds::CLUBMAN, 16, 13);
    app.camera.look_at_tile(12.0, 12.0);
    app.selection.set(vec![a, b]);
    draw(&mut app);

    // Over our Town Center the cursor offers garrison, and the click sends them in.
    let (px, py) = on_screen(&app, 12.5, 12.5, 24.0);
    assert!(matches!(app.hovered_target(px, py), Some(Target::Garrison)));
    let commands = app.sim.replay().commands.len();
    app.right_press(px, py);
    assert_eq!(app.sim.replay().commands.len(), commands + 1);
    for _ in 0..300 {
        app.sim.step();
        let w = app.sim.world();
        if [a, b]
            .iter()
            .all(|u| w.slot(*u).is_some_and(|s| w.inside[s.index()] == Some(tc)))
        {
            break;
        }
    }
    let w = app.sim.world();
    assert!(w.inside[w.slot(a).unwrap().index()] == Some(tc), "went in");
    draw(&mut app);
    assert!(
        app.selection.ids.is_empty(),
        "inside, they leave the selection"
    );
    let (px, py) = on_screen(&app, 16.5, 12.5, 4.0);
    assert!(
        selection::pick(&app.scene, &app.atlas, &app.camera, &app.sim, px, py).is_none(),
        "and cannot be picked where they stood"
    );

    // The building's panel shows them and lets them out.
    app.selection.set(vec![tc]);
    draw(&mut app);
    let out = button(&app, "ALL OUT");
    assert!(
        out.reason.starts_with("LET THE 2 INSIDE OUT"),
        "{}",
        out.reason
    );
    click(&mut app, &out);
    step(&mut app, 3);
    let w = app.sim.world();
    assert!(w.inside[w.slot(a).unwrap().index()].is_none());
    assert!(w.inside[w.slot(b).unwrap().index()].is_none());
    draw(&mut app);
    assert!(!button_enabled(&app, "ALL OUT"), "nobody inside now");

    // The Tool Age, for the palisade.
    let store = spawn(&mut app, kinds::STOREHOUSE, 6, 6);
    let _ = spawn(&mut app, kinds::BARRACKS, 6, 18);
    let _ = store;
    app.issue(CommandKind::Research {
        building: tc,
        tech: tech::AGE_TOOL,
    });
    step(&mut app, tech::info(tech::AGE_TOOL).unwrap().ticks() + 5);
    assert_eq!(app.sim.player(ME).unwrap().age, sim::Age::Tool);

    // A villager: DEFENCES opens the page, PALISADE arms the wall, a drag
    // places the run, with the villager sent to its first segment.
    let vill = spawn(&mut app, kinds::VILLAGER, 18, 8);
    app.selection.set(vec![vill]);
    draw(&mut app);
    assert!(app.hud.buttons.iter().all(|b| b.label != "PALISADE"));
    let defences = button(&app, "DEFENCES");
    click(&mut app, &defences);
    assert!(app.defences);
    draw(&mut app);
    assert!(button_enabled(&app, "BACK"));
    assert!(app.hud.buttons.iter().all(|b| b.label != "HOUSE"));
    let palisade = button(&app, "PALISADE");
    click(&mut app, &palisade);
    assert_eq!(app.build_mode, Some(kinds::PALISADE_WALL));
    assert!(!app.defences);
    let wood = app.sim.player(ME).unwrap().stockpile[1];
    let commands = app.sim.replay().commands.len();
    let (px, py) = on_screen(&app, 20.5, 8.5, 0.0);
    app.input.cursor = Some((px, py));
    app.left_press(px, py);
    assert_eq!(app.wall_from, Some((20, 8)));
    let (qx, qy) = on_screen(&app, 20.5, 12.5, 0.0);
    app.input.cursor = Some((qx, qy));
    draw(&mut app);
    assert!(
        !app.hud.sprites.is_empty() && app.run_status().starts_with("5 PALISADE WALL 25 WOOD"),
        "{}",
        app.run_status()
    );
    app.left_release(qx, qy);
    assert_eq!(
        app.sim.replay().commands.len(),
        commands + 5,
        "five segments"
    );
    assert_eq!(app.build_mode, None, "placed; shift would keep placing");
    step(&mut app, 3);
    assert_eq!(app.sim.player(ME).unwrap().stockpile[1], wood - 25);
    let sites: Vec<_> = app
        .sim
        .world()
        .slots()
        .filter(|s| app.sim.world().kind[s.index()] == kinds::PALISADE_WALL)
        .collect();
    assert_eq!(sites.len(), 5);
    assert!(matches!(
        app.sim.world().order[app.sim.world().slot(vill).unwrap().index()],
        Order::Build { .. }
    ));
}

fn button_enabled(app: &App, label: &str) -> bool {
    app.hud
        .buttons
        .iter()
        .any(|b| b.label == label && b.enabled)
}

/// The units answer as they are told: a click on one of the player's
/// villagers is its selection call and a right-click order its bark,
/// each in the same input call, before a tick has passed; a panel
/// button clicks and a greyed one buzzes; and twelve villagers on one
/// tree are at most four chop voices at once, each at its own pitch.
///
/// REQ: UX-AUDIO-01
/// REQ: UX-AUDIO-02
#[test]
fn units_answer_at_once_buttons_click_and_a_woodline_is_four_voices() {
    let mut app = app();
    two_sides(&mut app);
    app.clock.set_paused(true);
    let v = spawn(&mut app, kinds::VILLAGER, 16, 10);
    app.camera.look_at_tile(16.5, 10.5);
    draw(&mut app);
    let _ = plays(&mut app);
    // Click the middle of its sprite as drawn.
    let slot = app.sim.world().slot(v).unwrap().index() as u32;
    let sprite = app
        .scene
        .sprites
        .iter()
        .find(|s| s.slot == slot && !s.screen)
        .cloned()
        .expect("the villager is drawn");
    let (x0, y0) = app.camera.to_window(sprite.x, sprite.y);
    let z = app.camera.zoom();
    let (px, py) = (x0 + sprite.w * z * 0.5, y0 + sprite.h * z * 0.6);
    app.left_press(px, py);
    app.left_release(px, py);
    assert_eq!(app.selection.ids, vec![v], "the click selected it");
    let heard = plays(&mut app);
    assert!(
        heard
            .iter()
            .any(|p| p.cue == Cue::Select(Class::Villager) && p.bus == Bus::Voice),
        "{heard:?}"
    );
    let tick = app.sim.tick();
    let (qx, qy) = on_screen(&app, 20.5, 12.5, 0.0);
    app.right_press(qx, qy);
    let heard = plays(&mut app);
    assert_eq!(app.sim.tick(), tick, "no tick has passed");
    assert!(
        heard.iter().any(|p| p.cue == Cue::Ack(Class::Villager)),
        "the bark: {heard:?}"
    );
    // The panel: HOUSE is affordable and clicks; a building the age
    // refuses buzzes and does not click.
    draw(&mut app);
    let house = button(&app, "HOUSE");
    click(&mut app, &house);
    let heard = plays(&mut app);
    assert!(
        heard
            .iter()
            .any(|p| p.cue == Cue::Click && p.bus == Bus::Ui),
        "{heard:?}"
    );
    app.do_action(Action::Cancel);
    let _ = plays(&mut app);
    draw(&mut app);
    let greyed = app
        .hud
        .buttons
        .iter()
        .find(|b| !b.enabled)
        .cloned()
        .expect("something the Stone Age refuses");
    click(&mut app, &greyed);
    let heard = plays(&mut app);
    assert!(heard.iter().any(|p| p.cue == Cue::Invalid), "{heard:?}");
    assert!(
        !heard.iter().any(|p| p.cue == Cue::Click),
        "a refusal is not a click"
    );
    // The woodline: twelve on one tree.
    let tree = spawn(&mut app, kinds::TREE, 24, 24);
    let ids: Vec<EntityId> = (0..12)
        .map(|k| spawn(&mut app, kinds::VILLAGER, 20 + k % 4, 20 + k / 4))
        .collect();
    app.issue(CommandKind::Gather { ids, node: tree });
    step(&mut app, 200);
    // Voices are busy in wall time; let the last ones end.
    std::thread::sleep(Duration::from_millis(300));
    let _ = plays(&mut app);
    // The app's own tick, which is where the events are heard.
    for _ in 0..sim::WORK_PERIOD {
        app.tick_once(Instant::now());
    }
    let heard = plays(&mut app);
    let chops: Vec<&Play> = heard
        .iter()
        .filter(|p| p.cue == Cue::Work(Task::Chop))
        .collect();
    assert!(
        !chops.is_empty() && chops.len() <= audio::MAX_VOICES,
        "{} chops in one period: {heard:?}",
        chops.len()
    );
    assert!(chops.iter().all(|p| p.bus == Bus::World));
    assert!(
        chops.len() == 1 || chops.iter().any(|p| (p.rate - chops[0].rate).abs() > 1e-4),
        "the pitches differ"
    );
}

/// The four bus volumes are on the settings screen in steps of ten, kept
/// in the file and applied to the mixer at once.
#[test]
fn bus_volumes_are_set_on_the_settings_screen_and_kept() {
    let mut app = app();
    app.shell = Shell::Title;
    press(&mut app, ShellAction::Settings);
    press(&mut app, ShellAction::Volume(Bus::Music, -1));
    press(&mut app, ShellAction::Volume(Bus::Music, -1));
    assert_eq!(app.settings.volume(Bus::Music), 50);
    assert!((app.mixer.volume(Bus::Music) - 0.5).abs() < 1e-6);
    let text = std::fs::read_to_string(&app.settings_path).unwrap();
    assert_eq!(Settings::from_ron(&text).unwrap().volume(Bus::Music), 50);
    for _ in 0..12 {
        press(&mut app, ShellAction::Volume(Bus::Ui, -1));
    }
    assert_eq!(app.settings.volume(Bus::Ui), 0, "floors at silent");
    assert_eq!(app.mixer.volume(Bus::Ui), 0.0);
    press(&mut app, ShellAction::Volume(Bus::Ui, 1));
    assert_eq!(app.settings.volume(Bus::Ui), 10);
    press(&mut app, ShellAction::ResetSettings);
    assert_eq!(app.settings.volume(Bus::Music), 70, "the default");
}

/// The score follows the match: the Stone stem fades in at the start and
/// the field's bed sits under it; the Tool Age cross-fades the stems over
/// four seconds; six units fighting in view bring the combat stem in; and
/// the title has none of it.
#[test]
fn the_score_follows_the_age_and_the_fight_and_the_beds_the_ground() {
    let mut app = app();
    two_sides(&mut app);
    app.clock.set_paused(true);
    draw(&mut app);
    let fades = app.speaker.take_fades();
    assert!(
        fades.contains(&Fade {
            layer: Layer::Stem(Age::Stone),
            level: 1.0,
            ms: CROSSFADE_MS
        }),
        "{fades:?}"
    );
    let field = fades
        .iter()
        .find(|f| f.layer == Layer::Bed(Bed::Field))
        .expect("the field under the camera");
    assert!(
        field.level > 0.0 && field.level <= BED_GAIN,
        "{}",
        field.level
    );
    assert!(
        !fades
            .iter()
            .any(|f| f.layer == Layer::Bed(Bed::Surf) && f.level > 0.0),
        "no water here"
    );
    draw(&mut app);
    assert!(app.speaker.take_fades().is_empty(), "steady");
    // The Tool Age: the two buildings it asks for, then the research.
    let tc = spawn(&mut app, kinds::TOWN_CENTER, 12, 12);
    spawn(&mut app, kinds::STOREHOUSE, 16, 8);
    spawn(&mut app, kinds::BARRACKS, 16, 16);
    let age = tech::all()
        .iter()
        .find(|t| t.advances_age() == Some(Age::Tool))
        .expect("the Tool Age advance");
    app.issue(CommandKind::Research {
        building: tc,
        tech: age.id,
    });
    step(&mut app, age.seconds as u32 * 20 + 40);
    assert_eq!(app.sim.player(ME).unwrap().age, Age::Tool);
    draw(&mut app);
    let fades = app.speaker.take_fades();
    assert!(
        fades.contains(&Fade {
            layer: Layer::Stem(Age::Stone),
            level: 0.0,
            ms: CROSSFADE_MS
        }),
        "{fades:?}"
    );
    assert!(fades.contains(&Fade {
        layer: Layer::Stem(Age::Tool),
        level: 1.0,
        ms: CROSSFADE_MS
    }));
    // Six clubmen sent at an enemy in view.
    app.camera.look_at_tile(30.0, 30.0);
    let mine: Vec<EntityId> = (0..6)
        .map(|k| spawn(&mut app, kinds::CLUBMAN, 28 + k, 28))
        .collect();
    app.sim.issue(Command {
        player: 1,
        kind: CommandKind::Spawn {
            kind: kinds::CLUBMAN,
            pos: Vec2Fx::from_int(31, 31),
        },
    });
    step(&mut app, 3);
    let theirs = {
        let w = app.sim.world();
        w.slots()
            .find(|s| w.kind[s.index()] == kinds::CLUBMAN && w.owner[s.index()] == 1)
            .map(|s| w.id_at(s))
            .expect("the enemy")
    };
    app.issue(CommandKind::Attack {
        ids: mine,
        target: theirs,
    });
    step(&mut app, 10);
    draw(&mut app);
    let fades = app.speaker.take_fades();
    assert!(
        fades.contains(&Fade {
            layer: Layer::Combat,
            level: 1.0,
            ms: COMBAT_IN_MS
        }),
        "{fades:?}"
    );
    // The title: everything out.
    app.shell = Shell::Title;
    draw(&mut app);
    let fades = app.speaker.take_fades();
    for layer in [
        Layer::Stem(Age::Tool),
        Layer::Combat,
        Layer::Bed(Bed::Field),
    ] {
        assert!(
            fades.iter().any(|f| f.layer == layer && f.level == 0.0),
            "{layer:?} out: {fades:?}"
        );
    }
}

/// A hint comes in context and is drawn, its count goes to the settings
/// file at once, the second time is the last, and HINTS OFF on the
/// settings screen ends them; a refused click that is short of a resource
/// flashes it on the bar and says so, where a refusal for another reason
/// only buzzes.
#[test]
fn hints_are_counted_in_the_file_and_a_short_click_flashes_the_bar() {
    let mut app = app();
    two_sides(&mut app);
    app.clock.set_paused(true);
    let v = spawn(&mut app, kinds::VILLAGER, 12, 12);
    // A villager is selected and nobody gathers, and the side has no
    // house at all: of the two hints due, being housed is the urgent one.
    app.selection.set(vec![v]);
    draw(&mut app);
    let hint = app.hud.hint.clone().expect("a hint");
    assert!(hint.contains("HOUSED"), "{hint}");
    let text = std::fs::read_to_string(&app.settings_path).unwrap();
    assert!(text.contains("housed"), "counted at once: {text}");
    assert_eq!(app.settings.hints_shown.get("housed"), Some(&1));
    draw(&mut app);
    assert!(app.hud.hint.is_some(), "still up");
    // Off on the settings screen: gone, and none come.
    app.shell_action(ShellAction::ToggleHints);
    assert!(!app.settings.hints);
    draw(&mut app);
    assert_eq!(app.hud.hint, None);
    app.shell_action(ShellAction::ToggleHints);
    assert!(app.settings.hints);
    // A player with wood and nothing else: the Town Center, refused for
    // a Government Centre, only buzzes; a villager at the Town Center,
    // short of food, flashes FOOD on the bar and plays the line.
    let mut poor = Simulation::new(
        1,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Flat,
                size: 48,
                players: 2,
            },
            starting_stockpile: [0, 250, 0, 0],
            wander: false,
            ..SimConfig::default()
        },
    );
    for (player, kind, x, y) in [
        (0, kinds::VILLAGER, 10, 10),
        (0, kinds::TOWN_CENTER, 14, 14),
        (1, kinds::VILLAGER, 40, 40),
    ] {
        poor.issue(Command {
            player,
            kind: CommandKind::Spawn {
                kind,
                pos: Vec2Fx::from_int(x, y),
            },
        });
    }
    for _ in 0..3 {
        poor.step();
    }
    app.sim = poor;
    app.prev_pos.clone_from(&app.sim.world().pos);
    let own = |app: &App, kind: u16| {
        let w = app.sim.world();
        w.slots()
            .find(|s| w.kind[s.index()] == kind && w.owner[s.index()] == 0)
            .map(|s| w.id_at(s))
            .unwrap()
    };
    let (v, tc) = (own(&app, kinds::VILLAGER), own(&app, kinds::TOWN_CENTER));
    app.selection.set(vec![v]);
    draw(&mut app);
    let _ = plays(&mut app);
    let tc_button = app
        .hud
        .buttons
        .iter()
        .find(|b| b.label == "TOWN CTR")
        .cloned()
        .expect("the town center button");
    assert!(
        !tc_button.enabled && tc_button.lacks == [false; 4],
        "{:?}",
        tc_button.lacks
    );
    click(&mut app, &tc_button);
    let heard = plays(&mut app);
    assert!(heard.iter().any(|p| p.cue == Cue::Invalid), "{heard:?}");
    assert!(app.flash.is_none(), "no flash for a rule");
    app.selection.set(vec![tc]);
    draw(&mut app);
    let _ = plays(&mut app);
    let train = app
        .hud
        .buttons
        .iter()
        .find(|b| b.label == "VILLAGER")
        .cloned()
        .expect("the villager button");
    assert!(
        !train.enabled && train.lacks == [true, false, false, false],
        "{:?}",
        train.lacks
    );
    click(&mut app, &train);
    let heard = plays(&mut app);
    assert!(heard.iter().any(|p| p.cue == Cue::Poor), "{heard:?}");
    assert_eq!(app.flash.map(|(_, l)| l), Some([true, false, false, false]));
    draw(&mut app);
    assert!(app.hud.sprites.len() > 40, "the bar drew with its flash");
}
