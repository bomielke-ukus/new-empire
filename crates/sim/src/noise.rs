//! Integer value noise for map generation.
//!
//! A lattice of random fixed-point values, bilinearly interpolated with a
//! smoothstep on the fractional part. No gradients, no floats — the same seed
//! is the same landscape on every machine, which is what lets a replay carry
//! just a seed instead of a map.

use crate::fx::Fx;
use crate::rng::Rng;

/// One octave of value noise over a tile grid.
pub struct ValueNoise {
    cols: usize,
    rows: usize,
    cell: i32,
    lattice: Vec<Fx>,
}

impl ValueNoise {
    /// Noise covering `width × height` tiles with lattice spacing `cell`.
    pub fn new(rng: &mut Rng, width: i32, height: i32, cell: i32) -> ValueNoise {
        let cell = cell.max(1);
        let cols = (width / cell + 2) as usize;
        let rows = (height / cell + 2) as usize;
        let lattice = (0..cols * rows).map(|_| rng.unit_fx()).collect();
        ValueNoise {
            cols,
            rows,
            cell,
            lattice,
        }
    }

    fn at(&self, cx: usize, cy: usize) -> Fx {
        self.lattice[cy.min(self.rows - 1) * self.cols + cx.min(self.cols - 1)]
    }

    /// Value in `[0, 1)` at a tile position. Positions outside the covered
    /// area clamp to the edge lattice.
    pub fn sample(&self, x: i32, y: i32) -> Fx {
        let x = x.max(0);
        let y = y.max(0);
        let cx = (x / self.cell) as usize;
        let cy = (y / self.cell) as usize;
        let tx = smoothstep(Fx::from_ratio(x % self.cell, self.cell));
        let ty = smoothstep(Fx::from_ratio(y % self.cell, self.cell));
        let top = Fx::lerp(self.at(cx, cy), self.at(cx + 1, cy), tx);
        let bottom = Fx::lerp(self.at(cx, cy + 1), self.at(cx + 1, cy + 1), tx);
        Fx::lerp(top, bottom, ty)
    }
}

/// `3t² − 2t³`, the classic ease.
fn smoothstep(t: Fx) -> Fx {
    t * t * (Fx::from_int(3) - Fx::TWO * t)
}

/// Two octaves of value noise summed: broad shape plus detail.
pub struct Fbm {
    coarse: ValueNoise,
    fine: ValueNoise,
}

impl Fbm {
    /// Coarse lattice of `cell` tiles, fine lattice of `cell / 3`.
    pub fn new(rng: &mut Rng, width: i32, height: i32, cell: i32) -> Fbm {
        Fbm {
            coarse: ValueNoise::new(rng, width, height, cell),
            fine: ValueNoise::new(rng, width, height, (cell / 3).max(2)),
        }
    }

    /// Value in `[0, 1)`: three parts coarse to one part fine.
    pub fn sample(&self, x: i32, y: i32) -> Fx {
        (self.coarse.sample(x, y) * 3 + self.fine.sample(x, y)) / 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothstep_endpoints_and_middle() {
        assert_eq!(smoothstep(Fx::ZERO), Fx::ZERO);
        assert_eq!(smoothstep(Fx::ONE), Fx::ONE);
        assert_eq!(smoothstep(Fx::HALF), Fx::HALF);
        assert!(smoothstep(Fx::from_ratio(1, 4)) < Fx::from_ratio(1, 4));
    }

    #[test]
    fn in_range_deterministic_and_continuous() {
        let mut a = Rng::new(5);
        let mut b = Rng::new(5);
        let na = ValueNoise::new(&mut a, 64, 64, 8);
        let nb = ValueNoise::new(&mut b, 64, 64, 8);
        let mut max_jump = Fx::ZERO;
        for y in 0..64 {
            for x in 0..64 {
                let v = na.sample(x, y);
                assert_eq!(v, nb.sample(x, y));
                assert!(v >= Fx::ZERO && v < Fx::ONE);
                if x > 0 {
                    max_jump = max_jump.max((v - na.sample(x - 1, y)).abs());
                }
            }
        }
        // Neighbouring tiles differ by a small fraction — the field is smooth.
        assert!(max_jump < Fx::from_ratio(1, 4), "{max_jump}");
        // Lattice points return the lattice value exactly.
        assert_eq!(na.sample(8, 16), na.at(1, 2));
    }

    #[test]
    fn fbm_in_range() {
        let mut r = Rng::new(1);
        let f = Fbm::new(&mut r, 100, 100, 16);
        for y in (0..100).step_by(7) {
            for x in (0..100).step_by(5) {
                let v = f.sample(x, y);
                assert!(v >= Fx::ZERO && v < Fx::ONE);
            }
        }
        assert_eq!(f.sample(-5, -5), f.sample(0, 0));
    }
}
