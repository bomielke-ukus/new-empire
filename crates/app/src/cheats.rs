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

/// The character a key types into one of the scenario editor's lines,
/// if the font has it: letters (capitals with shift), digits, space and
/// the punctuation the font draws, as a US keyboard lays them out.
pub fn text_char(code: KeyCode, shift: bool) -> Option<char> {
    use KeyCode::*;
    if let Some(c) = typed(code) {
        return Some(if shift || c == ' ' {
            c
        } else {
            c.to_ascii_lowercase()
        });
    }
    let digit = |d: char, shifted: Option<char>| match (shift, shifted) {
        (true, Some(s)) => Some(s),
        (true, None) => None,
        (false, _) => Some(d),
    };
    match code {
        Digit0 => digit('0', Some(')')),
        Digit1 => digit('1', Some('!')),
        Digit2 => digit('2', None),
        Digit3 => digit('3', None),
        Digit4 => digit('4', None),
        Digit5 => digit('5', Some('%')),
        Digit6 => digit('6', None),
        Digit7 => digit('7', None),
        Digit8 => digit('8', None),
        Digit9 => digit('9', Some('(')),
        Minus if !shift => Some('-'),
        Equal => Some(if shift { '+' } else { '=' }),
        Comma if !shift => Some(','),
        Period if !shift => Some('.'),
        Slash => Some(if shift { '?' } else { '/' }),
        Semicolon => Some(if shift { ':' } else { ';' }),
        Quote if !shift => Some('\''),
        BracketLeft if !shift => Some('['),
        BracketRight if !shift => Some(']'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything the editor's lines take is in the font.
    #[test]
    fn the_editors_lines_take_only_what_the_font_draws() {
        use KeyCode::*;
        let font = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 :/-.%+()?,![]=;'";
        let keys = [
            KeyA,
            KeyZ,
            Digit0,
            Digit1,
            Digit2,
            Digit5,
            Digit9,
            Minus,
            Equal,
            Comma,
            Period,
            Slash,
            Semicolon,
            Quote,
            BracketLeft,
            BracketRight,
            Space,
            Backquote,
        ];
        for shift in [false, true] {
            for k in keys {
                if let Some(c) = text_char(k, shift) {
                    assert!(
                        font.contains(c.to_ascii_uppercase()),
                        "{k:?} {shift}: {c:?}"
                    );
                }
            }
        }
        assert_eq!(text_char(KeyA, false), Some('a'));
        assert_eq!(text_char(KeyA, true), Some('A'));
        assert_eq!(text_char(Slash, true), Some('?'));
        assert_eq!(text_char(Digit2, true), None, "@ is not in the font");
    }

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
