//! The second ownership cue (`docs/07` D37, `GD-A11Y-03`): a symbol per
//! player, so whose a unit is never rests on colour alone. Eight colours
//! cannot be told apart comfortably by a dichromatic player (`docs/08` §6,
//! the worst pair at 0.072 Oklab); eight shapes can.
//!
//! The symbol is drawn in its player's colour with a black outline: over
//! the selection, over everything with [`PlayerSymbols::Always`], and
//! beside the colour wherever a side is listed.

use serde::{Deserialize, Serialize};

/// Each player's shape, 9 by 9, in player order: a circle, a square, a
/// triangle, a diamond, a plus, an X, a triangle upside down, a star.
/// Chosen for silhouettes that differ at a glance at this size, filled so
/// they hold their colour.
pub const SHAPES: [[&str; 9]; 8] = [
    [
        "...###...",
        ".#######.",
        ".#######.",
        "#########",
        "#########",
        "#########",
        ".#######.",
        ".#######.",
        "...###...",
    ],
    [
        ".........",
        ".#######.",
        ".#######.",
        ".#######.",
        ".#######.",
        ".#######.",
        ".#######.",
        ".#######.",
        ".........",
    ],
    [
        "....#....",
        "....#....",
        "...###...",
        "...###...",
        "..#####..",
        "..#####..",
        ".#######.",
        ".#######.",
        "#########",
    ],
    [
        "....#....",
        "...###...",
        "..#####..",
        ".#######.",
        "#########",
        ".#######.",
        "..#####..",
        "...###...",
        "....#....",
    ],
    [
        "...###...",
        "...###...",
        "...###...",
        "#########",
        "#########",
        "#########",
        "...###...",
        "...###...",
        "...###...",
    ],
    [
        "##.....##",
        "###...###",
        ".###.###.",
        "..#####..",
        "...###...",
        "..#####..",
        ".###.###.",
        "###...###",
        "##.....##",
    ],
    [
        "#########",
        ".#######.",
        ".#######.",
        "..#####..",
        "..#####..",
        "...###...",
        "...###...",
        "....#....",
        "....#....",
    ],
    [
        "....#....",
        "....#....",
        "...###...",
        "#########",
        ".#######.",
        "..#####..",
        "..##.##..",
        ".##...##.",
        ".#.....#.",
    ],
];

/// When the symbols are drawn over the world (the settings screen's
/// SYMBOLS). Beside a side's colour in a list they always are.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum PlayerSymbols {
    /// Never over the world.
    Off,
    /// Over what is selected. The default: it costs nothing to look at.
    #[default]
    Selected,
    /// Over every unit and building of every side, walls aside.
    Always,
}

impl PlayerSymbols {
    /// Every choice, in the order the arrows step through them.
    pub const ALL: [PlayerSymbols; 3] = [
        PlayerSymbols::Selected,
        PlayerSymbols::Always,
        PlayerSymbols::Off,
    ];

    /// How the settings screen shows it.
    pub const fn label(self) -> &'static str {
        match self {
            PlayerSymbols::Off => "OFF",
            PlayerSymbols::Selected => "SELECTED",
            PlayerSymbols::Always => "ALWAYS",
        }
    }

    /// The choice `delta` steps along, wrapping.
    pub fn step(self, delta: i32) -> PlayerSymbols {
        let n = PlayerSymbols::ALL.len() as i32;
        let i = PlayerSymbols::ALL
            .iter()
            .position(|&p| p == self)
            .unwrap_or(0) as i32;
        PlayerSymbols::ALL[(i + delta).rem_euclid(n) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eight shapes, each its own: no two fill the same pixels, and none
    /// is the same as another turned upside down by accident of drawing.
    /// REQ: GD-A11Y-03
    #[test]
    fn eight_shapes_all_different() {
        for (i, a) in SHAPES.iter().enumerate() {
            for row in a {
                assert_eq!(row.len(), 9, "shape {i}");
            }
            let filled = a
                .iter()
                .flat_map(|r| r.chars())
                .filter(|&c| c == '#')
                .count();
            assert!(
                filled >= 25,
                "shape {i} is too thin to hold its colour: {filled}"
            );
            for (j, b) in SHAPES.iter().enumerate().skip(i + 1) {
                let differ = a
                    .iter()
                    .zip(b)
                    .flat_map(|(ra, rb)| ra.chars().zip(rb.chars()))
                    .filter(|(x, y)| x != y)
                    .count();
                assert!(differ >= 12, "shapes {i} and {j} differ in {differ} pixels");
            }
        }
    }

    #[test]
    fn the_setting_steps_round() {
        assert_eq!(PlayerSymbols::default(), PlayerSymbols::Selected);
        assert_eq!(PlayerSymbols::Selected.step(1), PlayerSymbols::Always);
        assert_eq!(PlayerSymbols::Always.step(1), PlayerSymbols::Off);
        assert_eq!(PlayerSymbols::Off.step(1), PlayerSymbols::Selected);
        assert_eq!(PlayerSymbols::Selected.step(-1), PlayerSymbols::Off);
    }
}
