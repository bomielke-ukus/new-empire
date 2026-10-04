//! Cheat codes (`docs/02` `GD-CHEAT-01`): Enter in a match opens a line,
//! the code typed into it and Enter again gives 1000 of a resource. The
//! code becomes a `CommandKind::Cheat` like any order, so the replay
//! holds it; the simulation ignores one a computer opponent issues.

use sim::kinds::Resource;
use winit::keyboard::KeyCode;

/// The codes, and what each gives.
pub const CODES: [(&str, Resource); 4] = [
    ("BOUNTIFUL HARVEST", Resource::Food),
    ("MIGHTY OAK", Resource::Wood),
    ("SOLID ROCK", Resource::Stone),
    ("MIDAS TOUCH", Resource::Gold),
];

/// The longest line the player may type.
pub const MAX_LEN: usize = 24;

/// The resource a typed line asks for, if it is a code: case and the
/// spaces around and between the words do not matter.
pub fn lookup(typed: &str) -> Option<Resource> {
    let words: Vec<String> = typed.split_whitespace().map(|w| w.to_uppercase()).collect();
    let line = words.join(" ");
    CODES
        .iter()
        .find(|(code, _)| *code == line)
        .map(|(_, r)| *r)
}

/// The character a key types into the line, if any: letters and space.
pub fn typed(code: KeyCode) -> Option<char> {
    use KeyCode::*;
    Some(match code {
        KeyA => 'A',
        KeyB => 'B',
        KeyC => 'C',
        KeyD => 'D',
        KeyE => 'E',
        KeyF => 'F',
        KeyG => 'G',
        KeyH => 'H',
        KeyI => 'I',
        KeyJ => 'J',
        KeyK => 'K',
        KeyL => 'L',
        KeyM => 'M',
        KeyN => 'N',
        KeyO => 'O',
        KeyP => 'P',
        KeyQ => 'Q',
        KeyR => 'R',
        KeyS => 'S',
        KeyT => 'T',
        KeyU => 'U',
        KeyV => 'V',
        KeyW => 'W',
        KeyX => 'X',
        KeyY => 'Y',
        KeyZ => 'Z',
        Space => ' ',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REQ: GD-CHEAT-01
    #[test]
    fn each_code_names_one_resource_and_spacing_does_not_matter() {
        for r in Resource::ALL {
            assert_eq!(CODES.iter().filter(|(_, c)| *c == r).count(), 1, "{r:?}");
        }
        for (code, r) in CODES {
            assert!(code.len() <= MAX_LEN, "{code}");
            assert_eq!(lookup(code), Some(r));
        }
        assert_eq!(lookup("  midas   touch "), Some(Resource::Gold));
        assert_eq!(lookup("MIDAS"), None);
        assert_eq!(lookup(""), None);
        assert_eq!(typed(KeyCode::KeyQ), Some('Q'));
        assert_eq!(typed(KeyCode::Digit1), None);
    }
}
