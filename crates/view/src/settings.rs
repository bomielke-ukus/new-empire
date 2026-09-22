//! The player's settings (`docs/06` M6, `GD-A11Y-02`): the HUD size, edge
//! scrolling, the window mode and the general key bindings, kept in a
//! file the app owns and edited on the settings screen. The command
//! letters on the panels are the HUD's tables (`hud.rs`) and are not
//! rebound here; a general key may not take one of them.

use crate::hud::command_letters;
use audio::Bus;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A key the app answers to outside the command panels.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Control {
    /// Pan the camera up while held.
    PanUp,
    /// Pan down.
    PanDown,
    /// Pan left.
    PanLeft,
    /// Pan right.
    PanRight,
    /// Pause and resume.
    Pause,
    /// Halve the game speed.
    Slower,
    /// Double the game speed.
    Faster,
    /// One zoom level in.
    ZoomIn,
    /// One zoom level out.
    ZoomOut,
    /// Cycle the HUD size.
    HudSize,
    /// The controls overlay.
    Help,
    /// Save the match.
    QuickSave,
    /// Edge scrolling on and off.
    EdgeScroll,
    /// The camera to the Town Center.
    Home,
    /// The camera to the next idle villager.
    NextIdle,
    /// Dismiss the selection.
    Dismiss,
    /// Whose eyes a replay is seen through.
    Eyes,
}

impl Control {
    /// Every control, in the order the screen lists them.
    pub const ALL: [Control; 17] = [
        Control::PanUp,
        Control::PanDown,
        Control::PanLeft,
        Control::PanRight,
        Control::Pause,
        Control::Slower,
        Control::Faster,
        Control::ZoomIn,
        Control::ZoomOut,
        Control::HudSize,
        Control::Help,
        Control::QuickSave,
        Control::EdgeScroll,
        Control::Home,
        Control::NextIdle,
        Control::Dismiss,
        Control::Eyes,
    ];

    /// What it does, for the screen and the overlay.
    pub const fn name(self) -> &'static str {
        match self {
            Control::PanUp => "PAN UP",
            Control::PanDown => "PAN DOWN",
            Control::PanLeft => "PAN LEFT",
            Control::PanRight => "PAN RIGHT",
            Control::Pause => "PAUSE",
            Control::Slower => "SLOWER",
            Control::Faster => "FASTER",
            Control::ZoomIn => "ZOOM IN",
            Control::ZoomOut => "ZOOM OUT",
            Control::HudSize => "HUD SIZE",
            Control::Help => "CONTROLS OVERLAY",
            Control::QuickSave => "QUICK SAVE",
            Control::EdgeScroll => "EDGE SCROLL ON, OFF",
            Control::Home => "HOME: THE TOWN CENTER",
            Control::NextIdle => "NEXT IDLE VILLAGER",
            Control::Dismiss => "DISMISS",
            Control::Eyes => "EYES, IN A REPLAY",
        }
    }

    /// The key it answers to out of the box, by the name `winit` prints.
    pub const fn default_key(self) -> &'static str {
        match self {
            Control::PanUp => "ArrowUp",
            Control::PanDown => "ArrowDown",
            Control::PanLeft => "ArrowLeft",
            Control::PanRight => "ArrowRight",
            Control::Pause => "Space",
            Control::Slower => "BracketLeft",
            Control::Faster => "BracketRight",
            Control::ZoomIn => "Equal",
            Control::ZoomOut => "Minus",
            Control::HudSize => "F2",
            Control::Help => "F1",
            Control::QuickSave => "F5",
            Control::EdgeScroll => "F3",
            Control::Home => "Home",
            Control::NextIdle => "Period",
            Control::Dismiss => "Delete",
            Control::Eyes => "Tab",
        }
    }

    /// Whether it pans the camera: read while held, not on the press.
    pub const fn pans(self) -> bool {
        matches!(
            self,
            Control::PanUp | Control::PanDown | Control::PanLeft | Control::PanRight
        )
    }
}

/// The HUD sizes the player can pick, on top of the display's scale.
pub const UI_SCALES: [f32; 3] = [1.0, 1.5, 2.0];

/// Everything the settings file holds. Fields missing from an older file
/// take their defaults, so a file never fails to load for being old.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The HUD size: one of [`UI_SCALES`].
    pub ui_scale: f32,
    /// Whether the cursor at the window's edge pans the camera.
    pub edge_scroll: bool,
    /// Borderless fullscreen rather than a window.
    pub fullscreen: bool,
    /// A key per control, by the key's name as `winit` prints it
    /// (`KeyW`, `F5`, `BracketRight`). A control left out keeps its
    /// default.
    pub bindings: BTreeMap<Control, String>,
    /// Each bus's volume in percent, in [`Bus::ALL`] order.
    pub volumes: [u8; 4],
    /// Whether the first-time hints show (`docs/03` §7).
    pub hints: bool,
    /// How often each hint has been shown, by its name; at most twice.
    pub hints_shown: BTreeMap<String, u8>,
    /// Panel letters moved to other keys (`GD-A11Y-02`), by the letter,
    /// each to a key by the name `winit` prints. A letter left out
    /// answers to its own key. Every letter keeps its meaning on every
    /// panel; only the key that presses it moves.
    pub letters: BTreeMap<char, String>,
}

/// The volumes out of the box: everything full, the music under it.
pub const DEFAULT_VOLUMES: [u8; 4] = [100, 100, 100, 70];
/// A step on the settings screen, percent.
pub const VOLUME_STEP: i32 = 10;

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            ui_scale: 1.0,
            edge_scroll: true,
            fullscreen: false,
            bindings: Control::ALL
                .iter()
                .map(|c| (*c, c.default_key().to_string()))
                .collect(),
            volumes: DEFAULT_VOLUMES,
            hints: true,
            hints_shown: BTreeMap::new(),
            letters: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// The key a control answers to.
    pub fn key(&self, control: Control) -> &str {
        self.bindings
            .get(&control)
            .map_or(control.default_key(), |k| k.as_str())
    }

    /// The control a key is bound to, if any.
    pub fn control(&self, key: &str) -> Option<Control> {
        Control::ALL.into_iter().find(|c| self.key(*c) == key)
    }

    /// Binds a key to a control. Refused, with the reason: a key another
    /// control holds, a letter the command panels use, Escape, which is
    /// the menu, and a digit, which is a control group.
    pub fn bind(&mut self, control: Control, key: &str) -> Result<(), String> {
        if key == "Escape" {
            return Err("ESCAPE IS THE MENU".to_string());
        }
        if key.starts_with("Digit") {
            return Err("DIGITS ARE THE CONTROL GROUPS".to_string());
        }
        // The pan keys may take a panel letter's key: they are held, not
        // pressed, and they win in the key handler, so binding WASD to pan
        // takes `A` from attack-move (`docs/07` D26) until the letter is
        // moved to another key.
        if self.letter_for(key).is_some() && !control.pans() {
            return Err(format!("{} IS A COMMAND KEY ON THE PANELS", pretty(key)));
        }
        if let Some(other) = self.control(key).filter(|o| *o != control) {
            return Err(format!("{} IS {}", pretty(key), other.name()));
        }
        self.bindings.insert(control, key.to_string());
        Ok(())
    }

    /// The key a panel letter answers to (`GD-A11Y-02`).
    pub fn letter_key(&self, letter: char) -> String {
        self.letters
            .get(&letter)
            .cloned()
            .unwrap_or_else(|| format!("Key{letter}"))
    }

    /// The panel letter a key presses, if any.
    pub fn letter_for(&self, key: &str) -> Option<char> {
        command_letters()
            .into_iter()
            .find(|l| self.letter_key(*l) == key)
    }

    /// A panel letter's key as a screen shows it.
    pub fn letter_shown(&self, letter: char) -> String {
        pretty(&self.letter_key(letter))
    }

    /// Moves a panel letter to a key. A key another letter holds is
    /// swapped with it, so every letter keeps a key of its own and no
    /// panel can have two commands on one key. Refused, with the reason:
    /// Escape, a digit, and a key a general control holds.
    pub fn bind_letter(&mut self, letter: char, key: &str) -> Result<(), String> {
        if !command_letters().contains(&letter) {
            return Err(format!("{letter} IS NOT A PANEL LETTER"));
        }
        if key == "Escape" {
            return Err("ESCAPE IS THE MENU".to_string());
        }
        if key.starts_with("Digit") {
            return Err("DIGITS ARE THE CONTROL GROUPS".to_string());
        }
        if let Some(c) = self.control(key) {
            return Err(format!("{} IS {}", pretty(key), c.name()));
        }
        let old = self.letter_key(letter);
        if let Some(other) = self.letter_for(key).filter(|o| *o != letter) {
            self.set_letter(other, &old);
        }
        self.set_letter(letter, key);
        Ok(())
    }

    fn set_letter(&mut self, letter: char, key: &str) {
        if key == format!("Key{letter}") {
            self.letters.remove(&letter);
        } else {
            self.letters.insert(letter, key.to_string());
        }
    }

    /// A bus's volume, percent.
    pub fn volume(&self, bus: Bus) -> u8 {
        self.volumes[bus.index()]
    }

    /// A bus's volume up or down by [`VOLUME_STEP`] per step, from silent
    /// to full.
    pub fn step_volume(&mut self, bus: Bus, steps: i32) {
        let v = i32::from(self.volumes[bus.index()]) + steps * VOLUME_STEP;
        self.volumes[bus.index()] = v.clamp(0, 100) as u8;
    }

    /// Everything back to the defaults.
    pub fn reset(&mut self) {
        *self = Settings::default();
    }

    /// The next HUD size along, wrapping.
    pub fn cycle_scale(&mut self, delta: i32) {
        let n = UI_SCALES.len() as i32;
        let i = UI_SCALES
            .iter()
            .position(|s| (*s - self.ui_scale).abs() < 0.01)
            .unwrap_or(0) as i32;
        self.ui_scale = UI_SCALES[(i + delta).rem_euclid(n) as usize];
    }

    /// Pretty RON, since a player may open the file.
    pub fn to_ron(&self) -> Result<String, String> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| e.to_string())
    }

    /// Reads settings; whatever the file lacks takes its default.
    pub fn from_ron(text: &str) -> Result<Settings, String> {
        ron::from_str(text).map_err(|e| e.to_string())
    }
}

/// A key's name as a screen shows it: `KeyW` is `W`, `ArrowUp` is `UP`,
/// `BracketRight` is `]`.
pub fn pretty(key: &str) -> String {
    let bare = key
        .strip_prefix("Key")
        .or_else(|| key.strip_prefix("Digit"))
        .or_else(|| key.strip_prefix("Arrow"))
        .unwrap_or(key);
    match bare {
        "BracketLeft" => "[".to_string(),
        "BracketRight" => "]".to_string(),
        "Equal" => "=".to_string(),
        "Minus" => "-".to_string(),
        "Period" => ".".to_string(),
        "Comma" => ",".to_string(),
        "Slash" => "/".to_string(),
        "Semicolon" => ";".to_string(),
        "NumpadAdd" => "NUM +".to_string(),
        "NumpadSubtract" => "NUM -".to_string(),
        "PageUp" => "PAGE UP".to_string(),
        "PageDown" => "PAGE DOWN".to_string(),
        other => other.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults bind every control to its own key, none of them a
    /// letter the panels use; a key cannot be given to two controls, nor
    /// a command letter, Escape or a digit to any, and the reason says so.
    ///
    /// REQ: GD-A11Y-02
    #[test]
    fn every_control_has_its_own_key_and_a_key_is_refused_with_a_reason() {
        let mut s = Settings::default();
        let mut seen = std::collections::BTreeSet::new();
        for c in Control::ALL {
            assert!(seen.insert(s.key(c).to_string()), "{c:?} shares a key");
            assert_eq!(s.control(s.key(c)), Some(c));
            if let Some(l) = s.key(c).strip_prefix("Key") {
                let l = l.chars().next().unwrap();
                assert!(
                    !command_letters().contains(&l),
                    "{c:?} defaults to the command key {l}"
                );
            }
        }
        assert_eq!(s.key(Control::Pause), "Space");
        assert_eq!(s.bind(Control::Pause, "F6"), Ok(()));
        assert_eq!(s.key(Control::Pause), "F6");
        assert_eq!(s.control("F6"), Some(Control::Pause));
        assert_eq!(s.control("Space"), None, "the old key is free");
        let err = s.bind(Control::Faster, "F6").unwrap_err();
        assert_eq!(err, "F6 IS PAUSE");
        assert_eq!(s.bind(Control::Faster, "F6"), Err(err));
        assert!(s
            .bind(Control::Faster, "KeyH")
            .unwrap_err()
            .contains("COMMAND KEY"));
        assert!(s
            .bind(Control::Faster, "Escape")
            .unwrap_err()
            .contains("MENU"));
        assert!(s
            .bind(Control::Faster, "Digit3")
            .unwrap_err()
            .contains("GROUPS"));
        assert_eq!(
            s.key(Control::Faster),
            "BracketRight",
            "a refusal changes nothing"
        );
        assert_eq!(
            s.bind(Control::Faster, "ArrowUp"),
            Err("UP IS PAN UP".to_string())
        );
        // The arrows pan out of the box, so `A` is free for attack-move
        // (`UX-CMD-02`); WASD is a binding away.
        assert_eq!(Settings::default().key(Control::PanLeft), "ArrowLeft");
        assert_eq!(s.bind(Control::PanLeft, "KeyA"), Ok(()));
        assert_eq!(
            s.bind(Control::Pause, "F6"),
            Ok(()),
            "rebinding to itself is fine"
        );
        s.reset();
        assert_eq!(s, Settings::default());
    }

    /// The file round-trips, an older file without a field loads with
    /// the default, the HUD size cycles through the three sizes, and keys
    /// read as a screen shows them.
    #[test]
    fn the_file_round_trips_and_an_old_file_takes_defaults_for_what_it_lacks() {
        let mut s = Settings::default();
        s.cycle_scale(1);
        assert_eq!(s.ui_scale, 1.5);
        s.cycle_scale(1);
        s.cycle_scale(1);
        assert_eq!(s.ui_scale, 1.0, "wraps");
        s.cycle_scale(-1);
        assert_eq!(s.ui_scale, 2.0);
        s.edge_scroll = false;
        s.fullscreen = true;
        s.bind(Control::QuickSave, "F9").unwrap();
        let text = s.to_ron().unwrap();
        assert!(text.contains("F9"));
        assert_eq!(Settings::from_ron(&text).unwrap(), s);
        let old = Settings::from_ron("(ui_scale: 1.5)").unwrap();
        assert_eq!(old.ui_scale, 1.5);
        assert!(old.edge_scroll);
        assert_eq!(old.key(Control::Pause), "Space");
        assert_eq!(old.volumes, DEFAULT_VOLUMES, "a file from before the buses");
        assert!(
            old.hints && old.hints_shown.is_empty(),
            "and before the hints"
        );
        s.hints = false;
        s.hints_shown.insert("gather".to_string(), 2);
        s.step_volume(Bus::Music, -2);
        assert_eq!(s.volume(Bus::Music), 50);
        s.step_volume(Bus::Music, -9);
        assert_eq!(s.volume(Bus::Music), 0, "floors at silent");
        s.step_volume(Bus::Music, 1);
        assert_eq!(s.volume(Bus::Music), 10);
        s.step_volume(Bus::Ui, 3);
        assert_eq!(s.volume(Bus::Ui), 100, "caps at full");
        assert_eq!(Settings::from_ron(&s.to_ron().unwrap()).unwrap(), s);
        assert!(Settings::from_ron("(ui_scale: \"big\")").is_err());
        assert_eq!(pretty("KeyW"), "W");
        assert_eq!(pretty("ArrowUp"), "UP");
        assert_eq!(pretty("BracketRight"), "]");
        assert_eq!(pretty("Space"), "SPACE");
        assert_eq!(pretty("F12"), "F12");
        assert_eq!(pretty("Digit4"), "4");
    }

    /// Every panel letter can be moved to another key (`GD-A11Y-02`): a
    /// key another letter holds is swapped, so no two letters share one;
    /// a general key, Escape and a digit are refused; and the general
    /// keys may then take a key a letter has left.
    ///
    /// REQ: GD-A11Y-02
    #[test]
    fn panel_letters_move_to_other_keys_and_swap_rather_than_collide() {
        let mut s = Settings::default();
        assert_eq!(s.letter_key('H'), "KeyH");
        assert_eq!(s.letter_for("KeyH"), Some('H'));
        assert_eq!(s.bind_letter('H', "F6"), Ok(()));
        assert_eq!(s.letter_for("F6"), Some('H'));
        assert_eq!(s.letter_for("KeyH"), None, "H's old key is free");
        assert_eq!(s.letter_shown('H'), "F6");
        // Onto a key another letter holds: the two swap.
        assert_eq!(s.bind_letter('B', "F6"), Ok(()));
        assert_eq!(s.letter_key('B'), "F6");
        assert_eq!(s.letter_key('H'), "KeyB");
        let keys: std::collections::BTreeSet<String> = command_letters()
            .into_iter()
            .map(|l| s.letter_key(l))
            .collect();
        assert_eq!(keys.len(), command_letters().len(), "one key per letter");
        assert_eq!(
            s.bind_letter('H', "Space"),
            Err("SPACE IS PAUSE".to_string())
        );
        assert!(s.bind_letter('H', "Escape").is_err());
        assert!(s.bind_letter('H', "Digit4").is_err());
        assert!(s.bind_letter('W', "F7").is_err(), "W is no panel's");
        // A general key may take the key a letter left behind, and not
        // one a letter still holds.
        s.bind_letter('X', "F8").unwrap();
        assert_eq!(s.bind(Control::Faster, "KeyX"), Ok(()));
        assert!(s
            .bind(Control::Faster, "F8")
            .unwrap_err()
            .contains("COMMAND KEY"));
        // Moved back to its own key, a letter leaves the file.
        s.bind_letter('X', "KeyX").unwrap_err();
        s.bind(Control::Faster, "BracketRight").unwrap();
        s.bind_letter('X', "KeyX").unwrap();
        assert!(!s.letters.contains_key(&'X'));
        let text = s.to_ron().unwrap();
        assert_eq!(Settings::from_ron(&text).unwrap(), s);
        s.reset();
        assert!(s.letters.is_empty());
    }
}
