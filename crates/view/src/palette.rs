//! The 256-colour palette and per-player colour ramps.
//!
//! Sprites are stored as palette indices. Indices in [`PLAYER_RAMP`] are
//! remapped to the owning player's colours at draw time, so one set of art
//! serves every player. This is the Genie engine's trick, and it also keeps
//! independently produced art coherent: everything is drawn from the same 256
//! colours.

/// Index 0 is always transparent.
pub const TRANSPARENT: u8 = 0;
/// Outline black.
pub const BLACK: u8 = 1;
/// White.
pub const WHITE: u8 = 2;
/// Shadow: drawn as a translucent dark; the software rasteriser darkens
/// what is underneath and the GPU does the same.
pub const SHADOW: u8 = 3;

/// Browns: dark trunk, bark, tan, sand.
pub const BROWN_DARK: u8 = 10;
/// Mid brown.
pub const BROWN: u8 = 11;
/// Tan.
pub const TAN: u8 = 12;
/// Pale sand.
pub const SAND: u8 = 13;
/// Deep foliage green.
pub const GREEN_DARK: u8 = 20;
/// Foliage green.
pub const GREEN: u8 = 21;
/// Lit foliage.
pub const GREEN_LIGHT: u8 = 22;
/// Dark rock.
pub const GREY_DARK: u8 = 30;
/// Rock.
pub const GREY: u8 = 31;
/// Lit rock.
pub const GREY_LIGHT: u8 = 32;
/// Dull gold.
pub const GOLD_DARK: u8 = 40;
/// Gold.
pub const GOLD: u8 = 41;
/// Bright gold.
pub const GOLD_LIGHT: u8 = 42;
/// Berry red.
pub const RED: u8 = 50;
/// Blood red.
pub const RED_DARK: u8 = 51;
/// Skin.
pub const SKIN: u8 = 60;
/// Gazelle hide.
pub const HIDE: u8 = 61;

/// The eight indices remapped per player: `[base, light, dark, darker,
/// highlight, mid-dark, mid-light, desaturated]`.
pub const PLAYER_RAMP: core::ops::Range<u8> = 240..248;
/// Player base colour.
pub const P_BASE: u8 = 240;
/// Player colour, lighter.
pub const P_LIGHT: u8 = 241;
/// Player colour, darker.
pub const P_DARK: u8 = 242;
/// Player colour, much darker.
pub const P_DARKER: u8 = 243;
/// Player colour, highlight.
pub const P_HIGHLIGHT: u8 = 244;

/// Number of player colour rows in the palette texture, plus one neutral row.
pub const ROWS: usize = 9;

/// RGB base colours for the eight players. Chosen to stay distinct under
/// deuteranopia and protanopia simulation; verify again when art lands.
pub const PLAYER_COLOURS: [[u8; 3]; 8] = [
    [50, 90, 225],   // blue
    [215, 45, 45],   // red
    [45, 165, 65],   // green
    [235, 200, 45],  // yellow
    [45, 190, 205],  // cyan
    [190, 70, 195],  // magenta
    [150, 150, 150], // grey
    [240, 135, 35],  // orange
];

/// The neutral base palette, RGBA.
pub fn base() -> [[u8; 4]; 256] {
    let mut p = [[0u8; 4]; 256];
    p[SHADOW as usize] = [0, 0, 0, 96];
    let mut set = |i: u8, r: u8, g: u8, b: u8| p[i as usize] = [r, g, b, 255];
    set(BLACK, 18, 14, 12);
    set(WHITE, 245, 242, 235);
    set(BROWN_DARK, 78, 52, 30);
    set(BROWN, 125, 86, 48);
    set(TAN, 190, 160, 110);
    set(SAND, 222, 205, 160);
    set(GREEN_DARK, 38, 84, 40);
    set(GREEN, 62, 128, 56);
    set(GREEN_LIGHT, 110, 172, 84);
    set(GREY_DARK, 82, 82, 88);
    set(GREY, 130, 130, 136);
    set(GREY_LIGHT, 182, 182, 188);
    set(GOLD_DARK, 170, 125, 30);
    set(GOLD, 225, 180, 50);
    set(GOLD_LIGHT, 250, 225, 120);
    set(RED, 205, 45, 50);
    set(RED_DARK, 130, 25, 30);
    set(SKIN, 222, 180, 140);
    set(HIDE, 200, 160, 105);
    // Neutral (gaia) player ramp: greys, so unowned things never look owned.
    for (i, c) in ramp([160, 160, 160]).iter().enumerate() {
        p[PLAYER_RAMP.start as usize + i] = [c[0], c[1], c[2], 255];
    }
    p
}

/// The eight-colour ramp for a base colour.
pub fn ramp(base: [u8; 3]) -> [[u8; 3]; 8] {
    let scale = |c: [u8; 3], f: f32| -> [u8; 3] {
        let s = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
        [s(c[0]), s(c[1]), s(c[2])]
    };
    let lighten = |c: [u8; 3], f: f32| -> [u8; 3] {
        let s = |v: u8| (v as f32 + (255.0 - v as f32) * f).round() as u8;
        [s(c[0]), s(c[1]), s(c[2])]
    };
    let grey = ((base[0] as u32 + base[1] as u32 + base[2] as u32) / 3) as u8;
    let desat = [
        ((base[0] as u32 + grey as u32) / 2) as u8,
        ((base[1] as u32 + grey as u32) / 2) as u8,
        ((base[2] as u32 + grey as u32) / 2) as u8,
    ];
    [
        base,
        lighten(base, 0.35),
        scale(base, 0.7),
        scale(base, 0.45),
        lighten(base, 0.65),
        scale(base, 0.85),
        lighten(base, 0.18),
        desat,
    ]
}

/// The full palette texture: row 0 neutral, rows 1..=8 players 0..=7, each a
/// copy of the base palette with the player ramp substituted. `ROWS × 256`
/// RGBA pixels, row-major.
pub fn texture() -> Vec<[u8; 4]> {
    let base = base();
    let mut out = Vec::with_capacity(ROWS * 256);
    out.extend_from_slice(&base);
    for colour in PLAYER_COLOURS {
        let mut row = base;
        for (i, c) in ramp(colour).iter().enumerate() {
            row[PLAYER_RAMP.start as usize + i] = [c[0], c[1], c[2], 255];
        }
        out.extend_from_slice(&row);
    }
    out
}

/// Which palette row draws an entity owned by `owner`.
pub fn row_for_owner(owner: u8) -> u8 {
    if owner == sim::kinds::GAIA || owner as usize >= PLAYER_COLOURS.len() {
        0
    } else {
        owner + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_palette_has_transparent_zero_and_opaque_named_colours() {
        let p = base();
        assert_eq!(p[0], [0, 0, 0, 0]);
        for i in [
            BLACK, WHITE, BROWN, GREEN, GREY, GOLD, RED, SKIN, HIDE, P_BASE, P_DARK,
        ] {
            assert_eq!(p[i as usize][3], 255, "index {i} should be opaque");
        }
        assert!(p[SHADOW as usize][3] < 255);
    }

    #[test]
    fn texture_rows_differ_only_in_the_ramp() {
        let t = texture();
        assert_eq!(t.len(), ROWS * 256);
        for row in 1..ROWS {
            for i in 0..256usize {
                let same = t[i] == t[row * 256 + i];
                let in_ramp = PLAYER_RAMP.contains(&(i as u8));
                assert_eq!(!same, in_ramp, "row {row} index {i}");
            }
        }
        // Every player's base colour is distinct.
        let mut bases: Vec<_> = (1..ROWS).map(|r| t[r * 256 + P_BASE as usize]).collect();
        bases.dedup();
        assert_eq!(bases.len(), 8);
    }

    #[test]
    fn ramp_is_ordered() {
        let r = ramp([100, 50, 200]);
        assert_eq!(r[0], [100, 50, 200]);
        assert!(r[1][0] > r[0][0] && r[2][0] < r[0][0] && r[3][0] < r[2][0]);
    }

    #[test]
    fn owner_rows() {
        assert_eq!(row_for_owner(sim::kinds::GAIA), 0);
        assert_eq!(row_for_owner(0), 1);
        assert_eq!(row_for_owner(7), 8);
        assert_eq!(row_for_owner(8), 0);
    }
}
