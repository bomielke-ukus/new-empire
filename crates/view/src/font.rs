//! A 5×7 pixel font, drawn into the atlas as palette-indexed glyphs so text
//! goes through the same sprite pipeline as everything else. Digits, capitals
//! and the punctuation a HUD needs. Lowercase maps to capitals.

/// Glyph cell width including one column of spacing.
pub const ADVANCE: u32 = 6;
/// Glyph width.
pub const GLYPH_W: u32 = 5;
/// Glyph height.
pub const GLYPH_H: u32 = 7;

/// Characters the font contains, in atlas order.
pub const CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 :/-.%+()?,![]=;'";

/// Rows of a glyph, `#` for a lit pixel.
pub fn rows(c: char) -> [&'static str; 7] {
    match c.to_ascii_uppercase() {
        'A' => [
            " ### ", "#   #", "#   #", "#####", "#   #", "#   #", "#   #",
        ],
        'B' => [
            "#### ", "#   #", "#   #", "#### ", "#   #", "#   #", "#### ",
        ],
        'C' => [
            " ### ", "#   #", "#    ", "#    ", "#    ", "#   #", " ### ",
        ],
        'D' => [
            "#### ", "#   #", "#   #", "#   #", "#   #", "#   #", "#### ",
        ],
        'E' => [
            "#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#####",
        ],
        'F' => [
            "#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#    ",
        ],
        'G' => [
            " ### ", "#   #", "#    ", "# ###", "#   #", "#   #", " ####",
        ],
        'H' => [
            "#   #", "#   #", "#   #", "#####", "#   #", "#   #", "#   #",
        ],
        'I' => [
            " ### ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### ",
        ],
        'J' => [
            "  ###", "   # ", "   # ", "   # ", "   # ", "#  # ", " ##  ",
        ],
        'K' => [
            "#   #", "#  # ", "# #  ", "##   ", "# #  ", "#  # ", "#   #",
        ],
        'L' => [
            "#    ", "#    ", "#    ", "#    ", "#    ", "#    ", "#####",
        ],
        'M' => [
            "#   #", "## ##", "# # #", "# # #", "#   #", "#   #", "#   #",
        ],
        'N' => [
            "#   #", "##  #", "# # #", "#  ##", "#   #", "#   #", "#   #",
        ],
        'O' => [
            " ### ", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ",
        ],
        'P' => [
            "#### ", "#   #", "#   #", "#### ", "#    ", "#    ", "#    ",
        ],
        'Q' => [
            " ### ", "#   #", "#   #", "#   #", "# # #", "#  # ", " ## #",
        ],
        'R' => [
            "#### ", "#   #", "#   #", "#### ", "# #  ", "#  # ", "#   #",
        ],
        'S' => [
            " ####", "#    ", "#    ", " ### ", "    #", "    #", "#### ",
        ],
        'T' => [
            "#####", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ",
        ],
        'U' => [
            "#   #", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ",
        ],
        'V' => [
            "#   #", "#   #", "#   #", "#   #", "#   #", " # # ", "  #  ",
        ],
        'W' => [
            "#   #", "#   #", "#   #", "# # #", "# # #", "## ##", "#   #",
        ],
        'X' => [
            "#   #", "#   #", " # # ", "  #  ", " # # ", "#   #", "#   #",
        ],
        'Y' => [
            "#   #", "#   #", " # # ", "  #  ", "  #  ", "  #  ", "  #  ",
        ],
        'Z' => [
            "#####", "    #", "   # ", "  #  ", " #   ", "#    ", "#####",
        ],
        '0' => [
            " ### ", "#   #", "#  ##", "# # #", "##  #", "#   #", " ### ",
        ],
        '1' => [
            "  #  ", " ##  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### ",
        ],
        '2' => [
            " ### ", "#   #", "    #", "   # ", "  #  ", " #   ", "#####",
        ],
        '3' => [
            "#####", "   # ", "  #  ", "   # ", "    #", "#   #", " ### ",
        ],
        '4' => [
            "   # ", "  ## ", " # # ", "#  # ", "#####", "   # ", "   # ",
        ],
        '5' => [
            "#####", "#    ", "#### ", "    #", "    #", "#   #", " ### ",
        ],
        '6' => [
            "  ## ", " #   ", "#    ", "#### ", "#   #", "#   #", " ### ",
        ],
        '7' => [
            "#####", "    #", "   # ", "  #  ", " #   ", " #   ", " #   ",
        ],
        '8' => [
            " ### ", "#   #", "#   #", " ### ", "#   #", "#   #", " ### ",
        ],
        '9' => [
            " ### ", "#   #", "#   #", " ####", "    #", "   # ", " ##  ",
        ],
        ':' => [
            "     ", "  #  ", "  #  ", "     ", "  #  ", "  #  ", "     ",
        ],
        '/' => [
            "    #", "    #", "   # ", "  #  ", " #   ", "#    ", "#    ",
        ],
        '-' => [
            "     ", "     ", "     ", "#####", "     ", "     ", "     ",
        ],
        '.' => [
            "     ", "     ", "     ", "     ", "     ", " ##  ", " ##  ",
        ],
        '%' => [
            "##   ", "##  #", "   # ", "  #  ", " #   ", "#  ##", "   ##",
        ],
        '+' => [
            "     ", "  #  ", "  #  ", "#####", "  #  ", "  #  ", "     ",
        ],
        '[' => [
            "###  ", "#    ", "#    ", "#    ", "#    ", "#    ", "###  ",
        ],
        ']' => [
            "  ###", "    #", "    #", "    #", "    #", "    #", "  ###",
        ],
        '=' => [
            "     ", "     ", "#####", "     ", "#####", "     ", "     ",
        ],
        ';' => [
            "     ", " ##  ", " ##  ", "     ", " ##  ", "  #  ", " #   ",
        ],
        '(' => [
            "   # ", "  #  ", " #   ", " #   ", " #   ", "  #  ", "   # ",
        ],
        ')' => [
            " #   ", "  #  ", "   # ", "   # ", "   # ", "  #  ", " #   ",
        ],
        '?' => [
            " ### ", "#   #", "    #", "   # ", "  #  ", "     ", "  #  ",
        ],
        ',' => [
            "     ", "     ", "     ", "     ", " ##  ", " ##  ", " #   ",
        ],
        '!' => [
            "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "     ", "  #  ",
        ],
        '\'' => [
            "  ## ", "  ## ", "  #  ", "     ", "     ", "     ", "     ",
        ],
        _ => [
            "     ", "     ", "     ", "     ", "     ", "     ", "     ",
        ],
    }
}

/// True if the font has a glyph for `c` (after upper-casing).
pub fn has(c: char) -> bool {
    CHARS.contains(c.to_ascii_uppercase())
}

/// Pixel width of a string.
pub fn width(text: &str) -> u32 {
    text.chars().count() as u32 * ADVANCE
}

/// `text` broken into lines no wider than `max` pixels, at spaces; a word
/// longer than a line is cut.
pub fn wrap(text: &str, max: f32) -> Vec<String> {
    let per_line = ((max / ADVANCE as f32).floor() as usize).max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let word: String = word.chars().take(per_line).collect();
        let len = line.chars().count();
        if len > 0 && len + 1 + word.chars().count() > per_line {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_is_well_formed() {
        for c in CHARS.chars() {
            let r = rows(c);
            for (i, row) in r.iter().enumerate() {
                assert_eq!(row.len(), 5, "{c:?} row {i}");
                assert!(row.chars().all(|p| p == '#' || p == ' '), "{c:?} row {i}");
            }
            if c != ' ' {
                assert!(r.iter().any(|row| row.contains('#')), "{c:?} is blank");
            }
        }
        assert!(has('a') && has('Z') && has('7') && has('\'') && !has('#'));
        assert_eq!(width("FOOD 200"), 48);
        assert_eq!(rows('q'), rows('Q'));
    }

    #[test]
    fn glyphs_are_distinct() {
        let all: Vec<_> = CHARS.chars().filter(|&c| c != ' ').map(rows).collect();
        for a in 0..all.len() {
            for b in a + 1..all.len() {
                assert_ne!(all[a], all[b], "glyphs {a} and {b} collide");
            }
        }
    }
}
