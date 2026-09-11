//! Loading, baking and checking the indexed palette.
//!
//! The palette is the reason art from four sources looks like one game
//! (`docs/05` §2.4). Everything downstream — validation, quantisation,
//! placeholder generation — resolves colours through a baked [`Palette`].

use crate::colour::{ramp, simulate, Deficiency, Linear, Oklab, Srgb};
use serde::Deserialize;
use std::path::Path;

/// The first index of the reserved player-colour ramp.
pub const PLAYER_RAMP_START: u8 = 240;
/// How many indices the player-colour ramp occupies.
pub const PLAYER_RAMP_LEN: usize = 8;
/// Index 0 is transparent, everywhere, always.
pub const TRANSPARENT: u8 = 0;

#[derive(Deserialize, Debug)]
pub struct RampSpec {
    pub name: String,
    pub start: u8,
    pub steps: usize,
    pub dark: String,
    pub light: String,
    pub hue_shift: f64,
}

#[derive(Deserialize, Debug)]
pub struct SpecialSpec {
    pub index: u8,
    pub name: String,
    pub colour: String,
}

#[derive(Deserialize, Debug)]
pub struct PlayerSpec {
    pub name: String,
    pub dark: String,
    pub light: String,
    pub hue_shift: f64,
}

#[derive(Deserialize, Debug)]
pub struct PaletteSpec {
    pub name: String,
    pub neutral: RampSpec,
    pub materials: Vec<RampSpec>,
    pub reserve: (u8, u8),
    pub player_slot: (u8, u8),
    pub specials: Vec<SpecialSpec>,
    pub players: Vec<PlayerSpec>,
    pub min_player_separation: f64,
    pub min_first_four_separation: f64,
}

/// A baked palette: 256 resolved colours plus the eight owner ramps that
/// replace indices 240..=247 at draw time.
#[derive(Debug)]
pub struct Palette {
    pub name: String,
    pub entries: [Srgb; 256],
    /// Which named ramp each index belongs to, for error messages.
    pub owner: [Option<String>; 256],
    pub players: Vec<(String, [Srgb; PLAYER_RAMP_LEN])>,
    pub min_player_separation: f64,
    pub min_first_four_separation: f64,
}

impl PaletteSpec {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        ron::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Expands the spec into 256 entries, checking the index layout as it goes.
    ///
    /// Every index must be claimed exactly once by a ramp, the reserve or a
    /// special, so a typo in a `start` shows up as an overlap or a hole rather
    /// than as a colour that is quietly wrong six months later.
    pub fn bake(&self) -> Result<Palette, String> {
        let mut entries = [Srgb::default(); 256];
        const NONE: Option<String> = None;
        let mut owner: [Option<String>; 256] = [NONE; 256];

        owner[TRANSPARENT as usize] = Some("transparent".to_string());

        let place = |spec: &RampSpec,
                     entries: &mut [Srgb; 256],
                     owner: &mut [Option<String>; 256]|
         -> Result<(), String> {
            let dark = Srgb::parse(&spec.dark).map_err(|e| format!("{}: {e}", spec.name))?;
            let light = Srgb::parse(&spec.light).map_err(|e| format!("{}: {e}", spec.name))?;
            if spec.steps < 2 {
                return Err(format!("{}: a ramp needs at least two steps", spec.name));
            }
            let end = spec.start as usize + spec.steps;
            if end > 256 {
                return Err(format!(
                    "{}: starts at {} and runs {} steps, past the end of the palette",
                    spec.name, spec.start, spec.steps
                ));
            }
            for (offset, colour) in ramp(dark, light, spec.steps, spec.hue_shift)
                .into_iter()
                .enumerate()
            {
                let i = spec.start as usize + offset;
                if let Some(claimed) = &owner[i] {
                    return Err(format!(
                        "index {i} is claimed by both '{claimed}' and '{}'",
                        spec.name
                    ));
                }
                entries[i] = colour;
                owner[i] = Some(spec.name.clone());
            }
            Ok(())
        };

        place(&self.neutral, &mut entries, &mut owner)?;
        for m in &self.materials {
            place(m, &mut entries, &mut owner)?;
        }

        // The player slot is a contract, not a preference: the shader indexes
        // it by constant.
        if self.player_slot != (PLAYER_RAMP_START, PLAYER_RAMP_START + 7) {
            return Err(format!(
                "player_slot must be ({PLAYER_RAMP_START}, {}), found {:?}",
                PLAYER_RAMP_START + 7,
                self.player_slot
            ));
        }
        if self.players.len() != 8 {
            return Err(format!(
                "expected 8 player colours, found {}",
                self.players.len()
            ));
        }

        let mut players = Vec::with_capacity(8);
        for p in &self.players {
            let dark = Srgb::parse(&p.dark).map_err(|e| format!("player {}: {e}", p.name))?;
            let light = Srgb::parse(&p.light).map_err(|e| format!("player {}: {e}", p.name))?;
            let steps = ramp(dark, light, PLAYER_RAMP_LEN, p.hue_shift);
            let mut arr = [Srgb::default(); PLAYER_RAMP_LEN];
            arr.copy_from_slice(&steps);
            players.push((p.name.clone(), arr));
        }

        // Player one's ramp stands in for the reserved indices, so an
        // unremapped sprite is still legible rather than black.
        for offset in 0..PLAYER_RAMP_LEN {
            let i = PLAYER_RAMP_START as usize + offset;
            if let Some(claimed) = &owner[i] {
                return Err(format!("player index {i} is also claimed by '{claimed}'"));
            }
            entries[i] = players[0].1[offset];
            owner[i] = Some("player".to_string());
        }

        for (lo, hi) in [self.reserve] {
            for i in lo as usize..=hi as usize {
                if let Some(claimed) = &owner[i] {
                    return Err(format!("reserve index {i} is also claimed by '{claimed}'"));
                }
                // Reserved indices are the error colour until something claims
                // them, so using one by accident is loud.
                entries[i] = Srgb::parse("#ff00ff").unwrap();
                owner[i] = Some("reserve".to_string());
            }
        }

        for s in &self.specials {
            let i = s.index as usize;
            if let Some(claimed) = &owner[i] {
                return Err(format!(
                    "special '{}' at index {i} is also claimed by '{claimed}'",
                    s.name
                ));
            }
            entries[i] = Srgb::parse(&s.colour).map_err(|e| format!("{}: {e}", s.name))?;
            owner[i] = Some(s.name.clone());
        }

        if let Some(hole) = owner.iter().position(Option::is_none) {
            return Err(format!(
                "index {hole} is not claimed by any ramp, reserve or special; \
                 every index must be accounted for"
            ));
        }

        Ok(Palette {
            name: self.name.clone(),
            entries,
            owner,
            players,
            min_player_separation: self.min_player_separation,
            min_first_four_separation: self.min_first_four_separation,
        })
    }
}

/// One player pair that is too close together under some vision model.
pub struct Collision {
    pub vision: &'static str,
    pub a: String,
    pub b: String,
    pub distance: f64,
    /// The bar this pair had to clear. Pairs drawn from the first four owners
    /// are held to a higher one.
    pub required: f64,
}

/// Owners are assigned in palette order, so a 1v1 is blue against red and a
/// four-player game never reaches cyan. The first four carry the strong bar.
pub const FIRST_FOUR: usize = 4;

impl Palette {
    /// Is `index` part of the reserved player-colour ramp?
    pub fn is_player_index(index: u8) -> bool {
        (PLAYER_RAMP_START..PLAYER_RAMP_START + PLAYER_RAMP_LEN as u8).contains(&index)
    }

    /// The index of the closest palette entry to `target`, measured in Oklab.
    ///
    /// Excludes index 0 (transparent), the reserved player ramp and the
    /// specials: a render must never be quantised *into* player colour by
    /// accident, because that would repaint parts of the sprite per owner.
    pub fn nearest(&self, target: Srgb) -> u8 {
        let want = Oklab::from(Linear::from(target));
        let mut best = (1u8, f64::MAX);
        for i in 1..248u16 {
            let i = i as u8;
            if Self::is_player_index(i) {
                continue;
            }
            let d = want.distance(Oklab::from(Linear::from(self.entries[i as usize])));
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0
    }

    /// Checks that all eight owners stay tellable apart in normal vision and
    /// under both simulated deficiencies.
    ///
    /// Compares the ramps step for step rather than only at the midpoint,
    /// because a unit is mostly mid-ramp but its outline and highlight carry
    /// ownership too.
    pub fn player_collisions(&self) -> Vec<Collision> {
        let models: [(&'static str, Option<Deficiency>); 3] = [
            ("normal vision", None),
            ("protanopia", Some(Deficiency::Protanopia)),
            ("deuteranopia", Some(Deficiency::Deuteranopia)),
        ];
        // Steps 2..6 of 8: the body of the ramp, which is what fills a sprite.
        const BODY: std::ops::Range<usize> = 2..6;

        let mut out = Vec::new();
        for (vision, deficiency) in models {
            let seen = |c: Srgb| match deficiency {
                Some(d) => simulate(c, d),
                None => c,
            };
            for (i, (name_a, ramp_a)) in self.players.iter().enumerate() {
                for (skipped, (name_b, ramp_b)) in self.players.iter().skip(i + 1).enumerate() {
                    let worst = BODY
                        .map(|step| {
                            let a = Oklab::from(Linear::from(seen(ramp_a[step])));
                            let b = Oklab::from(Linear::from(seen(ramp_b[step])));
                            a.distance(b)
                        })
                        .fold(f64::MAX, f64::min);
                    let required = if i < FIRST_FOUR && i + 1 + skipped < FIRST_FOUR {
                        self.min_first_four_separation
                    } else {
                        self.min_player_separation
                    };
                    if worst < required {
                        out.push(Collision {
                            vision,
                            a: name_a.clone(),
                            b: name_b.clone(),
                            distance: worst,
                            required,
                        });
                    }
                }
            }
        }
        out.sort_by(|x, y| x.distance.partial_cmp(&y.distance).unwrap());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> PaletteSpec {
        PaletteSpec::load(Path::new("../../assets/palette/ancient.ron"))
            .expect("the shipped palette must parse")
    }

    #[test]
    fn shipped_palette_bakes_with_every_index_claimed() {
        let baked = spec().bake().expect("the shipped palette must bake");
        assert_eq!(baked.entries.len(), 256);
        assert!(baked.owner.iter().all(Option::is_some));
        assert_eq!(baked.players.len(), 8);
    }

    #[test]
    fn player_colours_survive_colour_blindness() {
        let baked = spec().bake().unwrap();
        let collisions = baked.player_collisions();
        let report: Vec<String> = collisions
            .iter()
            .map(|c| {
                format!(
                    "  {} vs {} under {}: {:.3}, needs {:.3}",
                    c.a, c.b, c.vision, c.distance, c.required
                )
            })
            .collect();
        assert!(
            collisions.is_empty(),
            "player colours too close:\n{}",
            report.join("\n")
        );
    }

    #[test]
    fn quantisation_never_lands_in_the_player_ramp() {
        let baked = spec().bake().unwrap();
        // Ask for each player colour back: it must resolve to an ordinary
        // index, never to 240..=247, or the sprite would recolour per owner.
        for (_, ramp) in &baked.players {
            for c in ramp {
                assert!(!Palette::is_player_index(baked.nearest(*c)));
            }
        }
    }

    #[test]
    fn overlapping_ramps_are_rejected() {
        let mut s = spec();
        s.materials[1].start = s.materials[0].start;
        assert!(s.bake().unwrap_err().contains("claimed by both"));
    }

    #[test]
    fn holes_in_the_index_layout_are_rejected() {
        let mut s = spec();
        s.reserve = (233, 237); // leaves 238 unclaimed
        assert!(s.bake().unwrap_err().contains("not claimed"));
    }
}
