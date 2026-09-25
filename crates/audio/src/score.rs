//! The score and the ambience (`docs/03` §6.1, `docs/04` §8, `docs/05`
//! §5.3): a stem per age under the match, cross-fading when the age
//! advances; the combat stem over it while a fight is in view; and an
//! ambient bed per kind of ground under the camera, low, looping and
//! positional (`docs/05` §5.1): toward the side of the view its ground
//! is on.
//! Both are pure: they decide the fades, and the app's device applies
//! them. Time is milliseconds on the caller's clock.

use crate::Bus;
use sim::Age;

/// A kind of ground, for the bed that plays over it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bed {
    /// Birds and leaves, under a canopy.
    Forest,
    /// Surf, by the water.
    Surf,
    /// Wind, over the desert.
    Wind,
    /// The open field: quiet.
    Field,
}

impl Bed {
    /// Every bed, in the order [`Ambience`] keeps them.
    pub const ALL: [Bed; 4] = [Bed::Forest, Bed::Surf, Bed::Wind, Bed::Field];

    const fn index(self) -> usize {
        self as usize
    }
}

/// A looping layer under the match.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    /// An age's stem.
    Stem(Age),
    /// The combat stem, over the age's.
    Combat,
    /// An ambient bed.
    Bed(Bed),
}

impl Layer {
    /// Every layer, in a stable order.
    pub fn all() -> Vec<Layer> {
        let mut out: Vec<Layer> = Age::ALL.into_iter().map(Layer::Stem).collect();
        out.push(Layer::Combat);
        out.extend(Bed::ALL.into_iter().map(Layer::Bed));
        out
    }

    /// The bus it plays on: the score on the music bus, the beds with the
    /// world they belong to.
    pub const fn bus(self) -> Bus {
        match self {
            Layer::Stem(_) | Layer::Combat => Bus::Music,
            Layer::Bed(_) => Bus::World,
        }
    }

    /// The name of its folder under `assets/sounds`: `stem-stone`,
    /// `stem-combat`, `bed-surf`.
    pub fn name(self) -> String {
        match self {
            Layer::Stem(a) => format!("stem-{}", crate::age_name(a)),
            Layer::Combat => "stem-combat".to_string(),
            Layer::Bed(Bed::Forest) => "bed-forest".to_string(),
            Layer::Bed(Bed::Surf) => "bed-surf".to_string(),
            Layer::Bed(Bed::Wind) => "bed-wind".to_string(),
            Layer::Bed(Bed::Field) => "bed-field".to_string(),
        }
    }

    /// The layer with that folder name, if any.
    pub fn from_name(name: &str) -> Option<Layer> {
        Layer::all().into_iter().find(|l| l.name() == name)
    }
}

/// A change to a layer: fade to `level` over `ms`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fade {
    /// Which.
    pub layer: Layer,
    /// 0 to 1, before the bus volume.
    pub level: f32,
    /// How long the fade takes.
    pub ms: u32,
    /// Where it sits, -1 left to 1 right, moved to over the same time:
    /// a bed toward its ground, the score in the middle.
    pub pan: f32,
}

/// An age's stem cross-fades into the next over this long (`docs/04` §8).
pub const CROSSFADE_MS: u32 = 4000;
/// The combat stem comes in this fast.
pub const COMBAT_IN_MS: u32 = 1000;
/// And leaves this slowly.
pub const COMBAT_OUT_MS: u32 = 3000;
/// The combat stem stays this long after the last fight in view, so a
/// battle with lulls is one piece of music.
pub const COMBAT_HOLD_MS: u64 = 6000;
/// This many units fighting in view bring the combat stem in
/// (`docs/04` §8).
pub const FIGHTING_FOR_COMBAT: usize = 6;
/// The beds follow the ground over this long.
pub const BED_FADE_MS: u32 = 2000;
/// The beds sit under everything: their full level is this.
pub const BED_GAIN: f32 = 0.35;

/// What the score is playing.
#[derive(Clone, Debug, Default)]
pub struct Score {
    age: Option<Age>,
    combat: bool,
    last_fight_ms: Option<u64>,
}

impl Score {
    /// The match this frame: the viewer's age, or none outside a match;
    /// how many units are fighting in view; and the time. The fades to
    /// apply, if anything changed.
    pub fn update(&mut self, age: Option<Age>, fighting: usize, now_ms: u64) -> Vec<Fade> {
        let mut out = Vec::new();
        if age != self.age {
            if let Some(old) = self.age {
                out.push(Fade {
                    layer: Layer::Stem(old),
                    level: 0.0,
                    ms: CROSSFADE_MS,
                    pan: 0.0,
                });
            }
            if let Some(new) = age {
                out.push(Fade {
                    layer: Layer::Stem(new),
                    level: 1.0,
                    ms: CROSSFADE_MS,
                    pan: 0.0,
                });
            }
            self.age = age;
        }
        if age.is_some() && fighting >= FIGHTING_FOR_COMBAT {
            self.last_fight_ms = Some(now_ms);
            if !self.combat {
                self.combat = true;
                out.push(Fade {
                    layer: Layer::Combat,
                    level: 1.0,
                    ms: COMBAT_IN_MS,
                    pan: 0.0,
                });
            }
        } else if self.combat {
            let quiet_for = now_ms.saturating_sub(self.last_fight_ms.unwrap_or(0));
            if age.is_none() || quiet_for >= COMBAT_HOLD_MS {
                self.combat = false;
                self.last_fight_ms = None;
                out.push(Fade {
                    layer: Layer::Combat,
                    level: 0.0,
                    ms: COMBAT_OUT_MS,
                    pan: 0.0,
                });
            }
        }
        out
    }

    /// The age whose stem is playing.
    pub fn age(&self) -> Option<Age> {
        self.age
    }

    /// Whether the combat stem is in.
    pub fn combat(&self) -> bool {
        self.combat
    }
}

/// The ground under the camera: what fraction of the explored tiles in
/// view is each kind. The fractions sum to one or less; the rest is
/// unexplored, which has no sound.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Ground {
    /// Under a canopy.
    pub forest: f32,
    /// Water, shallow or deep.
    pub water: f32,
    /// Desert and beach.
    pub sand: f32,
    /// Grass and dirt.
    pub open: f32,
    /// Where each kind lies across the view, in [`Bed::ALL`] order: the
    /// mean of its tiles' places, -1 the left edge to 1 the right.
    pub across: [f32; 4],
}

/// The beds' levels and places, following the ground.
#[derive(Clone, Debug, Default)]
pub struct Ambience {
    levels: [f32; 4],
    pans: [f32; 4],
}

/// A bed's target moves by at least this before it is worth a fade.
const BED_STEP: f32 = 0.05;
/// A bed's place moves by at least this before it is worth a fade.
const BED_PAN_STEP: f32 = 0.1;

impl Ambience {
    /// The ground under the camera this moment, or none outside a match.
    /// The fades to apply: a bed whose target or place moved enough, or
    /// to or from silence. A bed going quiet stays where it was.
    pub fn update(&mut self, ground: Option<Ground>) -> Vec<Fade> {
        let targets = ground.map_or([0.0; 4], |g| {
            [
                (g.forest * 2.0).min(1.0),
                (g.water * 2.0).min(1.0),
                (g.sand * 1.5).min(1.0),
                (g.open * 0.6).min(1.0),
            ]
            .map(|t| t * BED_GAIN)
        });
        let places = ground.map_or(self.pans, |g| {
            g.across.map(|x| x.clamp(-1.0, 1.0) * crate::PAN_WIDTH)
        });
        let mut out = Vec::new();
        for bed in Bed::ALL {
            let i = bed.index();
            let (have, want) = (self.levels[i], targets[i]);
            let place = if want > 0.0 { places[i] } else { self.pans[i] };
            let silence_changed = (have == 0.0) != (want == 0.0);
            let moved = want > 0.0 && (self.pans[i] - place).abs() >= BED_PAN_STEP;
            if silence_changed || (have - want).abs() >= BED_STEP || moved {
                self.levels[i] = want;
                self.pans[i] = place;
                out.push(Fade {
                    layer: Layer::Bed(bed),
                    level: want,
                    ms: BED_FADE_MS,
                    pan: place,
                });
            }
        }
        out
    }

    /// A bed's level now.
    pub fn level(&self, bed: Bed) -> f32 {
        self.levels[bed.index()]
    }

    /// Where a bed sits now, -1 left to 1 right.
    pub fn pan(&self, bed: Bed) -> f32 {
        self.pans[bed.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A match starts on its age's stem; the next age cross-fades the old
    /// stem out and the new one in over four seconds; six units fighting
    /// in view bring the combat stem in, fewer keep it for the hold and
    /// then let it go; leaving the match takes everything out.
    #[test]
    fn the_stem_follows_the_age_and_the_combat_stem_the_fight() {
        let mut s = Score::default();
        assert!(s.update(None, 0, 0).is_empty(), "nothing on the title");
        let start = s.update(Some(Age::Stone), 0, 1000);
        assert_eq!(
            start,
            vec![Fade {
                layer: Layer::Stem(Age::Stone),
                level: 1.0,
                ms: CROSSFADE_MS,
                pan: 0.0
            }]
        );
        assert!(s.update(Some(Age::Stone), 0, 2000).is_empty(), "steady");
        let up = s.update(Some(Age::Tool), 0, 3000);
        assert_eq!(up.len(), 2);
        assert!(up.contains(&Fade {
            layer: Layer::Stem(Age::Stone),
            level: 0.0,
            ms: CROSSFADE_MS,
            pan: 0.0
        }));
        assert!(up.contains(&Fade {
            layer: Layer::Stem(Age::Tool),
            level: 1.0,
            ms: CROSSFADE_MS,
            pan: 0.0
        }));
        assert_eq!(s.age(), Some(Age::Tool));
        assert!(
            s.update(Some(Age::Tool), FIGHTING_FOR_COMBAT - 1, 4000)
                .is_empty(),
            "five is a scuffle"
        );
        let fight = s.update(Some(Age::Tool), FIGHTING_FOR_COMBAT, 5000);
        assert_eq!(
            fight,
            vec![Fade {
                layer: Layer::Combat,
                level: 1.0,
                ms: COMBAT_IN_MS,
                pan: 0.0
            }]
        );
        assert!(s.combat());
        assert!(
            s.update(Some(Age::Tool), 0, 5000 + COMBAT_HOLD_MS - 1)
                .is_empty(),
            "held"
        );
        let calm = s.update(Some(Age::Tool), 0, 5000 + COMBAT_HOLD_MS);
        assert_eq!(
            calm,
            vec![Fade {
                layer: Layer::Combat,
                level: 0.0,
                ms: COMBAT_OUT_MS,
                pan: 0.0
            }]
        );
        assert!(!s.combat());
        s.update(Some(Age::Tool), 10, 20_000);
        let out = s.update(None, 0, 21_000);
        assert_eq!(out.len(), 2, "the stem and the combat stem both leave");
        assert!(out.iter().all(|f| f.level == 0.0));
        assert_eq!(s.age(), None);
        assert!(!s.combat());
    }

    /// The beds follow the ground: water brings the surf up, sand the wind,
    /// a canopy the birds, and the open field is quiet; a small move is not
    /// a fade, and no ground is silence.
    #[test]
    fn the_beds_follow_the_ground_under_the_camera() {
        let mut a = Ambience::default();
        assert!(a.update(None).is_empty());
        let shore = a.update(Some(Ground {
            forest: 0.0,
            water: 0.5,
            sand: 0.2,
            open: 0.3,
            ..Ground::default()
        }));
        let of = |bed: Bed| shore.iter().find(|f| f.layer == Layer::Bed(bed)).copied();
        assert_eq!(
            of(Bed::Surf).unwrap().level,
            BED_GAIN,
            "half the view is water: full surf"
        );
        assert!((of(Bed::Wind).unwrap().level - 0.3 * BED_GAIN).abs() < 1e-5);
        assert!((of(Bed::Field).unwrap().level - 0.18 * BED_GAIN).abs() < 1e-5);
        assert!(of(Bed::Forest).is_none(), "no canopy, no birds");
        assert!(shore.iter().all(|f| f.ms == BED_FADE_MS));
        let nudge = a.update(Some(Ground {
            forest: 0.0,
            water: 0.51,
            sand: 0.19,
            open: 0.3,
            ..Ground::default()
        }));
        assert!(nudge.is_empty(), "a small move is not a fade");
        let woods = a.update(Some(Ground {
            forest: 0.6,
            water: 0.0,
            sand: 0.0,
            open: 0.4,
            ..Ground::default()
        }));
        assert!(woods
            .iter()
            .any(|f| f.layer == Layer::Bed(Bed::Forest) && f.level == BED_GAIN));
        assert!(woods
            .iter()
            .any(|f| f.layer == Layer::Bed(Bed::Surf) && f.level == 0.0));
        assert_eq!(a.level(Bed::Surf), 0.0);
        let gone = a.update(None);
        assert!(gone.iter().all(|f| f.level == 0.0));
        assert_eq!(gone.len(), 2, "the two that were up");
        for l in Layer::all() {
            assert_eq!(Layer::from_name(&l.name()), Some(l), "{l:?}");
        }
        assert_eq!(Layer::Stem(Age::Iron).name(), "stem-iron");
        assert_eq!(Layer::Bed(Bed::Surf).bus(), Bus::World);
        assert_eq!(Layer::Combat.bus(), Bus::Music);
    }

    /// The beds are positional (`docs/05` §5.1): water on the left puts
    /// the surf on the left, as far as a world sound goes; the ground
    /// moving across the view moves the bed, a small shift does not; a
    /// bed going quiet stays where it was.
    #[test]
    fn a_bed_sits_toward_the_side_its_ground_is_on() {
        let mut a = Ambience::default();
        let shore = |across_water: f32| Ground {
            water: 0.5,
            open: 0.5,
            across: [0.0, across_water, 0.0, -across_water],
            ..Ground::default()
        };
        let left = a.update(Some(shore(-1.0)));
        let surf = |fades: &[Fade]| {
            fades
                .iter()
                .find(|f| f.layer == Layer::Bed(Bed::Surf))
                .copied()
        };
        assert_eq!(surf(&left).unwrap().pan, -crate::PAN_WIDTH);
        assert_eq!(
            a.pan(Bed::Field),
            crate::PAN_WIDTH,
            "the grass to the right"
        );
        assert!(a.update(Some(shore(-0.95))).is_empty(), "a small shift");
        let middle = a.update(Some(shore(0.0)));
        assert_eq!(surf(&middle).unwrap().pan, 0.0);
        assert_eq!(
            surf(&middle).unwrap().level,
            BED_GAIN,
            "only the place moved"
        );
        a.update(Some(shore(0.5)));
        let quiet = a.update(None);
        assert_eq!(surf(&quiet).unwrap().level, 0.0);
        assert_eq!(surf(&quiet).unwrap().pan, 0.5 * crate::PAN_WIDTH);
    }
}
