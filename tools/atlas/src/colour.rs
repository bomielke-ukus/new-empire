//! Colour space maths for the palette pipeline.
//!
//! Three jobs:
//!
//! 1. Interpolate ramps perceptually. Interpolating sRGB directly produces
//!    muddy midtones; Oklab is uniform enough that an eight-step ramp between
//!    two endpoints lands where an artist would put it.
//! 2. Hue-shift ramps. Shifting shadows cool and highlights warm across a ramp
//!    is the technique that stops pixel art looking like flat shading, and it
//!    is why the late-90s palettes read as "painted" rather than "computed".
//! 3. Simulate dichromatic vision, so `docs/05`'s requirement that player
//!    colours are "checked against deuteranopia and protanopia simulation
//!    before we commit" is an assertion in a test rather than a good intention.
//!
//! Floats are fine here. This is a build-time tool; nothing in `crates/sim`
//! ever sees a value produced by this module.

/// 8-bit sRGB, the form the palette is stored and written in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Srgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Srgb {
    /// Parses `#rrggbb` (the `#` is optional).
    pub fn parse(s: &str) -> Result<Self, String> {
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("'{s}' is not a #rrggbb colour"));
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
        Ok(Srgb {
            r: byte(0),
            g: byte(2),
            b: byte(4),
        })
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Linear-light RGB. All colour maths happens here or in Oklab.
#[derive(Clone, Copy, Debug, Default)]
pub struct Linear {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// Oklab: perceptually uniform, cheap, and well behaved under interpolation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Oklab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

fn srgb_to_linear_channel(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb_channel(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let c = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

impl From<Srgb> for Linear {
    fn from(c: Srgb) -> Self {
        Linear {
            r: srgb_to_linear_channel(c.r),
            g: srgb_to_linear_channel(c.g),
            b: srgb_to_linear_channel(c.b),
        }
    }
}

impl From<Linear> for Srgb {
    fn from(c: Linear) -> Self {
        Srgb {
            r: linear_to_srgb_channel(c.r),
            g: linear_to_srgb_channel(c.g),
            b: linear_to_srgb_channel(c.b),
        }
    }
}

impl From<Linear> for Oklab {
    fn from(c: Linear) -> Self {
        let l = 0.412_221_470_8 * c.r + 0.536_332_536_3 * c.g + 0.051_445_992_9 * c.b;
        let m = 0.211_903_498_2 * c.r + 0.680_699_545_1 * c.g + 0.107_396_956_6 * c.b;
        let s = 0.088_302_461_9 * c.r + 0.281_718_837_6 * c.g + 0.629_978_700_5 * c.b;
        let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
        Oklab {
            l: 0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
            a: 1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
            b: 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
        }
    }
}

impl From<Oklab> for Linear {
    fn from(c: Oklab) -> Self {
        let l = c.l + 0.396_337_777_4 * c.a + 0.215_803_757_3 * c.b;
        let m = c.l - 0.105_561_345_8 * c.a - 0.063_854_172_8 * c.b;
        let s = c.l - 0.089_484_177_5 * c.a - 1.291_485_548_0 * c.b;
        let (l, m, s) = (l * l * l, m * m * m, s * s * s);
        Linear {
            r: 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
            g: -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
            b: -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
        }
    }
}

impl Oklab {
    /// Perceptual distance. Roughly: 0.02 is "just noticeable on a sprite",
    /// 0.10 is "obviously a different colour across a battlefield".
    pub fn distance(self, other: Oklab) -> f64 {
        let (dl, da, db) = (self.l - other.l, self.a - other.a, self.b - other.b);
        (dl * dl + da * da + db * db).sqrt()
    }

    /// Chroma and hue, for hue-shifted ramp interpolation.
    fn to_polar(self) -> (f64, f64, f64) {
        (
            self.l,
            (self.a * self.a + self.b * self.b).sqrt(),
            self.b.atan2(self.a),
        )
    }

    fn from_polar(l: f64, chroma: f64, hue: f64) -> Self {
        Oklab {
            l,
            a: chroma * hue.cos(),
            b: chroma * hue.sin(),
        }
    }
}

/// Builds a ramp of `steps` colours from `dark` to `light`.
///
/// Lightness and chroma interpolate linearly in Oklab. Hue bows away from the
/// straight line by up to `hue_shift_deg` in the midtones and returns to the
/// authored hue at both ends, so the two colours the artist picked come back
/// exactly and only the interior is shifted. That bow is the pixel-art ramp
/// technique: a negative shift cools the midtones, a positive one warms them,
/// and either reads as light having a colour rather than shading being a
/// lightness slider.
pub fn ramp(dark: Srgb, light: Srgb, steps: usize, hue_shift_deg: f64) -> Vec<Srgb> {
    assert!(steps >= 2, "a ramp needs at least two steps");
    let (l0, c0, h0) = Oklab::from(Linear::from(dark)).to_polar();
    let (l1, c1, h1) = Oklab::from(Linear::from(light)).to_polar();

    // Take the short way round the hue circle, then add the authored shift.
    let mut dh = h1 - h0;
    if dh > std::f64::consts::PI {
        dh -= std::f64::consts::TAU;
    } else if dh < -std::f64::consts::PI {
        dh += std::f64::consts::TAU;
    }
    let shift = hue_shift_deg.to_radians();

    (0..steps)
        .map(|i| {
            let t = i as f64 / (steps - 1) as f64;
            let bow = (std::f64::consts::PI * t).sin();
            let lab = Oklab::from_polar(
                l0 + (l1 - l0) * t,
                c0 + (c1 - c0) * t,
                h0 + dh * t + shift * bow,
            );
            Srgb::from(Linear::from(lab))
        })
        .collect()
}

/// The two dichromacies that matter for player colour: no long-wavelength
/// cones (protanopia, ~1% of men) and no medium-wavelength cones
/// (deuteranopia, ~1.5% of men). Together they cover the overwhelming majority
/// of colour vision deficiency.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Deficiency {
    Protanopia,
    Deuteranopia,
}

impl Deficiency {
    pub fn name(self) -> &'static str {
        match self {
            Deficiency::Protanopia => "protanopia",
            Deficiency::Deuteranopia => "deuteranopia",
        }
    }
}

/// Simulates how `colour` appears to a dichromat, after Viénot, Brettel and
/// Mollon (1999): convert to LMS cone response, project the missing cone's
/// axis onto the remaining two, convert back.
pub fn simulate(colour: Srgb, deficiency: Deficiency) -> Srgb {
    let c = Linear::from(colour);
    let l = 17.8824 * c.r + 43.5161 * c.g + 4.11935 * c.b;
    let m = 3.45565 * c.r + 27.1554 * c.g + 3.86714 * c.b;
    let s = 0.0299566 * c.r + 0.184309 * c.g + 1.46709 * c.b;

    let (l, m, s) = match deficiency {
        Deficiency::Protanopia => (2.02344 * m - 2.52581 * s, m, s),
        Deficiency::Deuteranopia => (l, 0.494207 * l + 1.24827 * s, s),
    };

    Srgb::from(Linear {
        r: 0.080_944_478 * l - 0.130_504_409 * m + 0.116_721_066 * s,
        g: -0.010_248_534 * l + 0.054_019_486 * m - 0.113_614_708 * s,
        b: -0.000_365_294 * l - 0.004_121_615 * m + 0.693_511_405 * s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_oklab() {
        for hex in ["#000000", "#ffffff", "#7f3d1a", "#2e6fb7", "#c8b47a"] {
            let c = Srgb::parse(hex).unwrap();
            let back = Srgb::from(Linear::from(Oklab::from(Linear::from(c))));
            assert_eq!(c, back, "{hex} did not survive the round trip");
        }
    }

    #[test]
    fn rejects_malformed_hex() {
        assert!(Srgb::parse("#12345").is_err());
        assert!(Srgb::parse("#gggggg").is_err());
        assert!(Srgb::parse("123456").is_ok());
    }

    #[test]
    fn ramp_endpoints_are_exact_and_lightness_is_monotonic() {
        let dark = Srgb::parse("#241608").unwrap();
        let light = Srgb::parse("#f0dcb4").unwrap();
        let r = ramp(dark, light, 8, -12.0);
        assert_eq!(r.len(), 8);
        assert_eq!(r[0], dark);
        assert_eq!(r[7], light);
        for pair in r.windows(2) {
            let a = Oklab::from(Linear::from(pair[0])).l;
            let b = Oklab::from(Linear::from(pair[1])).l;
            assert!(b > a, "ramp lightness must increase: {a} then {b}");
        }
    }

    #[test]
    fn dichromats_cannot_tell_red_from_green() {
        // The sanity check on the simulation itself: pure red and pure green
        // are far apart in normal vision and close under deuteranopia.
        let red = Srgb::parse("#ff0000").unwrap();
        let green = Srgb::parse("#00ff00").unwrap();
        let normal = Oklab::from(Linear::from(red)).distance(Oklab::from(Linear::from(green)));
        let sim_red = simulate(red, Deficiency::Deuteranopia);
        let sim_green = simulate(green, Deficiency::Deuteranopia);
        let deutan =
            Oklab::from(Linear::from(sim_red)).distance(Oklab::from(Linear::from(sim_green)));
        assert!(
            deutan < normal * 0.5,
            "deuteranopia should collapse red and green: {normal} then {deutan}"
        );
    }

    #[test]
    fn greys_are_unchanged_by_dichromacy() {
        for deficiency in [Deficiency::Protanopia, Deficiency::Deuteranopia] {
            let grey = Srgb::parse("#808080").unwrap();
            let seen = simulate(grey, deficiency);
            let d = Oklab::from(Linear::from(grey)).distance(Oklab::from(Linear::from(seen)));
            assert!(d < 0.03, "{} shifted grey by {d}", deficiency.name());
        }
    }
}
