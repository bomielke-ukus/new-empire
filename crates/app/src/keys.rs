//! Key names: `winit`'s `KeyCode` as text, which is what the settings
//! file and the settings screen hold, and back again for the keys the
//! camera reads while they are held.

use winit::keyboard::KeyCode;

/// The name of a key as the settings hold it: `KeyW`, `F5`, `BracketRight`.
pub fn name(code: KeyCode) -> String {
    format!("{code:?}")
}

/// The key a name stands for, for every key a control may be bound to.
/// A name outside this list is not bindable.
pub fn code(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    Some(match name {
        "KeyA" => KeyA,
        "KeyB" => KeyB,
        "KeyC" => KeyC,
        "KeyD" => KeyD,
        "KeyE" => KeyE,
        "KeyF" => KeyF,
        "KeyG" => KeyG,
        "KeyH" => KeyH,
        "KeyI" => KeyI,
        "KeyJ" => KeyJ,
        "KeyK" => KeyK,
        "KeyL" => KeyL,
        "KeyM" => KeyM,
        "KeyN" => KeyN,
        "KeyO" => KeyO,
        "KeyP" => KeyP,
        "KeyQ" => KeyQ,
        "KeyR" => KeyR,
        "KeyS" => KeyS,
        "KeyT" => KeyT,
        "KeyU" => KeyU,
        "KeyV" => KeyV,
        "KeyW" => KeyW,
        "KeyX" => KeyX,
        "KeyY" => KeyY,
        "KeyZ" => KeyZ,
        "F1" => F1,
        "F2" => F2,
        "F3" => F3,
        "F4" => F4,
        "F5" => F5,
        "F6" => F6,
        "F7" => F7,
        "F8" => F8,
        "F9" => F9,
        "F10" => F10,
        "F11" => F11,
        "F12" => F12,
        "ArrowUp" => ArrowUp,
        "ArrowDown" => ArrowDown,
        "ArrowLeft" => ArrowLeft,
        "ArrowRight" => ArrowRight,
        "Space" => Space,
        "Tab" => Tab,
        "Home" => Home,
        "End" => End,
        "PageUp" => PageUp,
        "PageDown" => PageDown,
        "Insert" => Insert,
        "Delete" => Delete,
        "Backspace" => Backspace,
        "Enter" => Enter,
        "Minus" => Minus,
        "Equal" => Equal,
        "BracketLeft" => BracketLeft,
        "BracketRight" => BracketRight,
        "Semicolon" => Semicolon,
        "Quote" => Quote,
        "Comma" => Comma,
        "Period" => Period,
        "Slash" => Slash,
        "Backslash" => Backslash,
        "Backquote" => Backquote,
        "NumpadAdd" => NumpadAdd,
        "NumpadSubtract" => NumpadSubtract,
        "NumpadMultiply" => NumpadMultiply,
        "NumpadDivide" => NumpadDivide,
        "NumpadEnter" => NumpadEnter,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use view::Control;

    /// Every default key resolves to the key it names and back.
    #[test]
    fn names_and_codes_agree_for_every_default_key() {
        for c in Control::ALL {
            let n = c.default_key();
            let k = code(n).unwrap_or_else(|| panic!("{n} is not a bindable key"));
            assert_eq!(name(k), n);
        }
        assert_eq!(code("Escape"), None, "the menu key is not bindable");
        assert_eq!(code("KeyW"), Some(KeyCode::KeyW));
        assert_eq!(name(KeyCode::BracketRight), "BracketRight");
    }
}
