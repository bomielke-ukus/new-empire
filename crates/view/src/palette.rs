//! The 256-colour palette and per-player colour ramps.
//!
//! There is one palette in this project: `assets/palette/ancient.ron`, baked
//! by `atlas export --rust` into [`crate::palette_table`]. The art pipeline
//! validates sprites against it and this module draws them with it, so an
//! index means the same colour in both places. The named constants below
//! are *indices into that table*, chosen once here, so the placeholder art
//! and the HUD read as "timber, step 4" rather than as colours of their own.
//!
//! Sprites are stored as palette indices. Indices in [`PLAYER_RAMP`] are
//! remapped to the owning player's ramp at draw time, so one set of art
//! serves every player — the Genie engine's trick. Index 0 is transparent
//! and [`SHADOW`] is drawn translucent; everything else is opaque.

use crate::palette_table::{ENTRIES, PLAYERS, PLAYER_SLOT, RAMPS};

/// Index of step `step` of the ramp called `name`. Panics at compile time
/// (in tests) rather than at draw time if a name or step is wrong.
pub const fn index(name: &str, step: u8) -> u8 {
    let mut i = 0;
    while i < RAMPS.len() {
        let (n, start, len) = RAMPS[i];
        if str_eq(n, name) {
            assert!(step < len, "ramp step out of range");
            return start + step;
        }
        i += 1;
    }
    panic!("no such ramp in the palette table");
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Index 0 is always transparent.
pub const TRANSPARENT: u8 = 0;
/// Outline black.
pub const BLACK: u8 = index("outline", 0);
/// White.
pub const WHITE: u8 = index("neutral", 15);
/// Ground shadow: drawn as a translucent dark by both renderers.
pub const SHADOW: u8 = index("shadow", 0);

/// Dark timber, for trunks and shaded walls.
pub const BROWN_DARK: u8 = index("timber_dark", 3);
/// Timber.
pub const BROWN: u8 = index("timber", 4);
/// Mudbrick, the tan of a building's base.
pub const TAN: u8 = index("mudbrick", 5);
/// Pale sand.
pub const SAND: u8 = index("sand", 6);
/// Deep foliage.
pub const GREEN_DARK: u8 = index("foliage", 2);
/// Foliage.
pub const GREEN: u8 = index("foliage", 4);
/// Lit foliage.
pub const GREEN_LIGHT: u8 = index("foliage", 6);
/// Dark rock.
pub const GREY_DARK: u8 = index("granite", 2);
/// Rock.
pub const GREY: u8 = index("granite", 4);
/// Lit rock.
pub const GREY_LIGHT: u8 = index("granite", 6);
/// Dull gold.
pub const GOLD_DARK: u8 = index("gold", 3);
/// Gold.
pub const GOLD: u8 = index("gold", 5);
/// Bright gold.
pub const GOLD_LIGHT: u8 = index("gold", 7);
/// Berry red.
pub const RED: u8 = index("cloth_red", 5);
/// Blood red.
pub const RED_DARK: u8 = index("cloth_red", 2);
/// Skin.
pub const SKIN: u8 = index("skin_light", 5);
/// Gazelle hide.
pub const HIDE: u8 = index("hide_tan", 5);
/// The HUD's accent colour.
pub const UI_ACCENT: u8 = index("ui_accent", 0);

// Age materials, for the placeholder buildings' progression
// (`docs/02` §4, `docs/05` §3): thatch and timber, then mudbrick, then
// dressed limestone, then granite and iron.
/// Tool Age wall.
pub const MUDBRICK: u8 = index("mudbrick", 6);
/// Tool Age wall, in shadow.
pub const MUDBRICK_DARK: u8 = index("mudbrick_shadow", 4);
/// Thatched roof.
pub const THATCH: u8 = index("thatch", 5);
/// Thatch in shadow.
pub const THATCH_DARK: u8 = index("thatch", 3);
/// Bronze Age wall.
pub const LIMESTONE: u8 = index("limestone", 6);
/// Bronze Age wall, in shadow.
pub const LIMESTONE_DARK: u8 = index("limestone", 3);
/// Bronze trim.
pub const BRONZE: u8 = index("bronze", 5);
/// Iron trim.
pub const IRON: u8 = index("iron", 5);
/// Iron, in shadow.
pub const IRON_DARK: u8 = index("iron", 3);
/// Linen cloth.
pub const LINEN: u8 = index("linen", 6);
/// Tilled earth.
pub const DIRT: u8 = index("dirt", 4);
/// A furrow.
pub const DIRT_DARK: u8 = index("dirt", 2);

/// The eight indices remapped per player, dark to light.
pub const PLAYER_RAMP: core::ops::Range<u8> = PLAYER_SLOT.0..PLAYER_SLOT.1 + 1;
/// Player colour, much darker.
pub const P_DARKER: u8 = PLAYER_SLOT.0 + 1;
/// Player colour, darker.
pub const P_DARK: u8 = PLAYER_SLOT.0 + 2;
/// Player base colour.
pub const P_BASE: u8 = PLAYER_SLOT.0 + 4;
/// Player colour, lighter.
pub const P_LIGHT: u8 = PLAYER_SLOT.0 + 6;
/// Player colour, highlight.
pub const P_HIGHLIGHT: u8 = PLAYER_SLOT.0 + 7;

/// Number of player colour rows in the palette texture, plus one neutral row.
pub const ROWS: usize = 9;

/// Alpha of the shadow index, out of 255.
pub const SHADOW_ALPHA: u8 = 100;

/// Mid-ramp RGB of each player, for the minimap and HUD.
pub const PLAYER_COLOURS: [[u8; 3]; 8] = [
    PLAYERS[0].1[4],
    PLAYERS[1].1[4],
    PLAYERS[2].1[4],
    PLAYERS[3].1[4],
    PLAYERS[4].1[4],
    PLAYERS[5].1[4],
    PLAYERS[6].1[4],
    PLAYERS[7].1[4],
];

/// The neutral base palette, RGBA: every index as baked, with transparency
/// and shadow alpha applied, and the player slot filled with neutral greys
/// so unowned things never look owned.
pub fn base() -> [[u8; 4]; 256] {
    let mut p = [[0u8; 4]; 256];
    for (i, c) in ENTRIES.iter().enumerate() {
        p[i] = [c[0], c[1], c[2], 255];
    }
    p[TRANSPARENT as usize] = [0, 0, 0, 0];
    p[SHADOW as usize] = [0, 0, 0, SHADOW_ALPHA];
    for (k, i) in PLAYER_RAMP.enumerate() {
        let n = ENTRIES[index("neutral", 3 + k as u8) as usize];
        p[i as usize] = [n[0], n[1], n[2], 255];
    }
    p
}

/// The full palette texture: row 0 neutral, rows 1..=8 players 0..=7, each a
/// copy of the base palette with the player ramp substituted. `ROWS × 256`
/// RGBA pixels, row-major.
pub fn texture() -> Vec<[u8; 4]> {
    let base = base();
    let mut out = Vec::with_capacity(ROWS * 256);
    out.extend_from_slice(&base);
    for (_, ramp) in PLAYERS.iter() {
        let mut row = base;
        for (k, i) in PLAYER_RAMP.enumerate() {
            let c = ramp[k];
            row[i as usize] = [c[0], c[1], c[2], 255];
        }
        out.extend_from_slice(&row);
    }
    out
}

/// sRGB of an index in the neutral row, as floats in `0..=1`.
pub fn rgb_f32(idx: u8) -> [f32; 3] {
    let c = ENTRIES[idx as usize];
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    ]
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
    fn named_indices_resolve_and_are_opaque() {
        let p = base();
        assert_eq!(p[0], [0, 0, 0, 0]);
        for i in [
            BLACK, WHITE, BROWN, GREEN, GREY, GOLD, RED, SKIN, HIDE, TAN, SAND, UI_ACCENT, P_BASE,
            P_DARK,
        ] {
            assert_eq!(p[i as usize][3], 255, "index {i} should be opaque");
        }
        assert_eq!(p[SHADOW as usize], [0, 0, 0, SHADOW_ALPHA]);
        assert_eq!(index("neutral", 0), 1);
        assert_eq!(index("skin_light", 0), 17);
        assert_eq!(index("gold", 7), 168);
        assert_eq!(SHADOW, 239);
        assert_eq!(BLACK, 254);
    }

    #[test]
    fn table_and_source_agree_on_the_contract() {
        assert_eq!(PLAYER_SLOT, (240, 247));
        assert_eq!(PLAYER_RAMP, 240..248);
        assert_eq!(PLAYERS.len(), 8);
        assert_eq!(
            RAMPS.iter().map(|r| r.2 as usize).sum::<usize>(),
            256,
            "every index owned once"
        );
        let mut next = 0u8;
        for &(_, start, len) in RAMPS {
            assert_eq!(start, next, "ramps are contiguous in index order");
            next = next.wrapping_add(len);
        }
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
        let mut bases: Vec<_> = (1..ROWS).map(|r| t[r * 256 + P_BASE as usize]).collect();
        bases.dedup();
        assert_eq!(bases.len(), 8);
        // Ramps run dark to light.
        for (_, ramp) in PLAYERS.iter() {
            let lum = |c: [u8; 3]| c[0] as u32 + c[1] as u32 + c[2] as u32;
            assert!(lum(ramp[0]) < lum(ramp[7]));
        }
    }

    #[test]
    fn owner_rows() {
        assert_eq!(row_for_owner(sim::kinds::GAIA), 0);
        assert_eq!(row_for_owner(0), 1);
        assert_eq!(row_for_owner(7), 8);
        assert_eq!(row_for_owner(8), 0);
    }
}
