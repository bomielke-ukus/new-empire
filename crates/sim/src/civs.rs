//! The civilizations (`docs/02` §11, `docs/07` D32): two or three flat
//! numeric bonuses each, and what each may not have. No unique units, no
//! unique mechanics. A side's civilization is part of the match's
//! configuration ([`crate::SimConfig::civs`]); a match that names none
//! plays every side as it played before there were any.
//!
//! The spec's table names a few things the game does not have yet: guard
//! towers, a priest's second tier. Those bonuses and denials wait for
//! them; D32 says where each of the rest landed, and D33 the ships.

use crate::entity::KindId;
use crate::kinds::{self, Resource};
use crate::tech::{self, Age, Effect, TechId};
use serde::{Deserialize, Serialize};

/// One of the eight civilizations.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum Civ {
    /// Gold, chariots and priests; no Academy, no heavy cavalry.
    Egyptians = 0,
    /// Fast heavy infantry; no chariots, no horse archers.
    Greeks = 1,
    /// Quick villagers and archers; no Legion.
    Assyrians = 2,
    /// Strong walls and stone; no heavy cavalry, no Catapult.
    Babylonians = 3,
    /// Hunters and fast elephants; no Ballista.
    Persians = 4,
    /// Wood and cheap elephants; no stone walls.
    Phoenicians = 5,
    /// Cheap villagers and strong walls; no elephants, no Catapult.
    Shang = 6,
    /// Rich farms and quick siege; no Iron Age cavalry.
    Sumerians = 7,
}

/// The building set a civilization raises (`docs/02` §11, `docs/05`
/// §2.5): how its buildings look, and nothing else. Every civilization is
/// drawn in the one set there is until the others are made.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Architecture {
    /// Egyptian.
    Egyptian,
    /// Greek.
    Greek,
    /// Mesopotamian.
    Mesopotamian,
    /// East Asian.
    Asian,
}

/// One bonus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bonus {
    /// A technology's effect, from the start.
    Effect(Effect),
    /// Hit points of these kinds, in percent added.
    Health(&'static [KindId], i32),
    /// Speed of these kinds, in percent added.
    Speed(&'static [KindId], i32),
    /// How often these kinds strike, in percent added: +50 strikes three
    /// times for every two.
    Rate(&'static [KindId], i32),
    /// Cost of these kinds, in percent added; less is cheaper.
    Cost(&'static [KindId], i32),
    /// Food off a hunted animal, in percent added to the rate.
    Hunting(i32),
    /// Tiles added to a priest's reach for conversion.
    ConvertRange(i32),
    /// A technology an age before its time.
    Early(TechId, Age),
}

/// A civilization's bonuses and denials.
#[derive(Clone, Copy, Debug)]
pub struct CivInfo {
    /// Which.
    pub civ: Civ,
    /// Display name.
    pub name: &'static str,
    /// Its buildings' look.
    pub architecture: Architecture,
    /// What it has that others do not.
    pub bonuses: &'static [Bonus],
    /// Its bonuses, in words, for the setup screen.
    pub about: &'static str,
    /// Units and buildings it may not have.
    pub denied: &'static [KindId],
    /// Technologies it may not research.
    pub denied_techs: &'static [TechId],
}

const WALLS: &[KindId] = &[kinds::PALISADE_WALL, kinds::STONE_WALL, kinds::GATE];
const DEFENCES: &[KindId] = &[
    kinds::PALISADE_WALL,
    kinds::STONE_WALL,
    kinds::GATE,
    kinds::WATCH_TOWER,
];
const ARCHERS: &[KindId] = &[
    kinds::SLINGER,
    kinds::BOWMAN,
    kinds::CHARIOT_ARCHER,
    kinds::HORSE_ARCHER,
];
const SIEGE: &[KindId] = &[kinds::STONE_THROWER, kinds::CATAPULT, kinds::BALLISTA];
const SHIPS: &[KindId] = &[
    kinds::FISHING_BOAT,
    kinds::ARCHER_SHIP,
    kinds::WAR_GALLEY,
    kinds::CATAPULT_SHIP,
];

const TABLE: [CivInfo; 8] = [
    CivInfo {
        civ: Civ::Egyptians,
        name: "Egyptians",
        architecture: Architecture::Egyptian,
        bonuses: &[
            Bonus::Effect(Effect::GatherRate(Resource::Gold, 20)),
            Bonus::Health(&[kinds::CHARIOT_ARCHER], 33),
            Bonus::ConvertRange(2),
        ],
        about: "GOLD +20%, CHARIOTS +33% HP, PRIESTS +2 RANGE",
        denied: &[
            kinds::ACADEMY,
            kinds::HOPLITE,
            kinds::LEGIONARY,
            kinds::HEAVY_CAVALRY,
        ],
        denied_techs: &[tech::LEGION],
    },
    CivInfo {
        civ: Civ::Greeks,
        name: "Greeks",
        architecture: Architecture::Greek,
        bonuses: &[
            Bonus::Speed(&[kinds::HOPLITE, kinds::LEGIONARY], 25),
            Bonus::Early(tech::LEGION, Age::Bronze),
            Bonus::Speed(SHIPS, 30),
        ],
        about: "HOPLITES +25% SPEED, LEGION IN BRONZE, SHIPS +30% SPEED",
        denied: &[kinds::CHARIOT_ARCHER, kinds::HORSE_ARCHER],
        denied_techs: &[],
    },
    CivInfo {
        civ: Civ::Assyrians,
        name: "Assyrians",
        architecture: Architecture::Mesopotamian,
        bonuses: &[
            Bonus::Effect(Effect::VillagerSpeed(10)),
            Bonus::Rate(ARCHERS, 20),
        ],
        about: "VILLAGERS +10% SPEED, ARCHERS SHOOT 20% FASTER",
        denied: &[kinds::LEGIONARY],
        denied_techs: &[tech::LEGION],
    },
    CivInfo {
        civ: Civ::Babylonians,
        name: "Babylonians",
        architecture: Architecture::Mesopotamian,
        bonuses: &[
            Bonus::Health(DEFENCES, 60),
            Bonus::Effect(Effect::GatherRate(Resource::Stone, 20)),
        ],
        about: "WALLS AND TOWERS +60% HP, STONE +20%",
        denied: &[kinds::HEAVY_CAVALRY, kinds::CATAPULT],
        denied_techs: &[tech::TORSION],
    },
    CivInfo {
        civ: Civ::Persians,
        name: "Persians",
        architecture: Architecture::Mesopotamian,
        bonuses: &[Bonus::Hunting(30), Bonus::Speed(&[kinds::WAR_ELEPHANT], 50)],
        about: "HUNTING +30%, ELEPHANTS +50% SPEED",
        denied: &[kinds::BALLISTA],
        denied_techs: &[],
    },
    CivInfo {
        civ: Civ::Phoenicians,
        name: "Phoenicians",
        architecture: Architecture::Greek,
        bonuses: &[
            Bonus::Effect(Effect::GatherRate(Resource::Wood, 30)),
            Bonus::Cost(&[kinds::WAR_ELEPHANT], -25),
        ],
        about: "WOOD +30%, ELEPHANTS COST 25% LESS",
        denied: &[kinds::STONE_WALL],
        denied_techs: &[],
    },
    CivInfo {
        civ: Civ::Shang,
        name: "Shang",
        architecture: Architecture::Asian,
        bonuses: &[
            Bonus::Cost(&[kinds::VILLAGER], -30),
            Bonus::Health(WALLS, 100),
        ],
        about: "VILLAGERS COST 30% LESS, WALLS +100% HP",
        denied: &[kinds::WAR_ELEPHANT, kinds::CATAPULT],
        denied_techs: &[tech::TORSION],
    },
    CivInfo {
        civ: Civ::Sumerians,
        name: "Sumerians",
        architecture: Architecture::Egyptian,
        bonuses: &[
            Bonus::Effect(Effect::FarmYield(250)),
            Bonus::Rate(SIEGE, 50),
        ],
        about: "FARMS +100% FOOD, SIEGE SHOOTS 50% FASTER",
        denied: &[kinds::WAR_ELEPHANT, kinds::HORSE_ARCHER],
        denied_techs: &[],
    },
];

impl Civ {
    /// Every civilization, in the setup screen's order.
    pub const ALL: [Civ; 8] = [
        Civ::Egyptians,
        Civ::Greeks,
        Civ::Assyrians,
        Civ::Babylonians,
        Civ::Persians,
        Civ::Phoenicians,
        Civ::Shang,
        Civ::Sumerians,
    ];

    /// Its bonuses and denials.
    pub fn info(self) -> &'static CivInfo {
        &TABLE[self as usize]
    }

    /// Display name.
    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// Whether it may have `kind`.
    pub fn allows(self, kind: KindId) -> bool {
        !self.info().denied.contains(&kind)
    }

    /// Whether it may research `tech`.
    pub fn allows_tech(self, tech: TechId) -> bool {
        !self.info().denied_techs.contains(&tech)
    }

    /// Hit points of `kind`, in percent added.
    pub fn health_pct(self, kind: KindId) -> i32 {
        self.sum(|b| match *b {
            Bonus::Health(ks, pct) if ks.contains(&kind) => pct,
            _ => 0,
        })
    }

    /// Speed of `kind`, in percent added.
    pub fn speed_pct(self, kind: KindId) -> i32 {
        self.sum(|b| match *b {
            Bonus::Speed(ks, pct) if ks.contains(&kind) => pct,
            _ => 0,
        })
    }

    /// Rate of striking for `kind`, in percent added.
    pub fn rate_pct(self, kind: KindId) -> i32 {
        self.sum(|b| match *b {
            Bonus::Rate(ks, pct) if ks.contains(&kind) => pct,
            _ => 0,
        })
    }

    /// Cost of `kind`, in percent added.
    pub fn cost_pct(self, kind: KindId) -> i32 {
        self.sum(|b| match *b {
            Bonus::Cost(ks, pct) if ks.contains(&kind) => pct,
            _ => 0,
        })
    }

    /// Food off a hunted animal, in percent added.
    pub fn hunting_pct(self) -> i32 {
        self.sum(|b| match *b {
            Bonus::Hunting(pct) => pct,
            _ => 0,
        })
    }

    /// Tiles added to a priest's reach.
    pub fn convert_range(self) -> i32 {
        self.sum(|b| match *b {
            Bonus::ConvertRange(n) => n,
            _ => 0,
        })
    }

    /// The age `tech` comes in for it.
    pub fn tech_age(self, tech: TechId, normally: Age) -> Age {
        self.info()
            .bonuses
            .iter()
            .find_map(|b| match *b {
                Bonus::Early(t, age) if t == tech => Some(age),
                _ => None,
            })
            .unwrap_or(normally)
    }

    fn sum(self, f: impl Fn(&Bonus) -> i32) -> i32 {
        self.info().bonuses.iter().map(f).sum()
    }
}

/// `value` with `pct` percent added, rounded down, never below one.
pub(crate) fn scaled(value: i32, pct: i32) -> i32 {
    if pct == 0 || value == 0 {
        value
    } else {
        (value * (100 + pct) / 100).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_in_order_and_every_civ_differs() {
        for (i, c) in Civ::ALL.iter().enumerate() {
            assert_eq!(*c as usize, i);
            assert_eq!(c.info().civ, *c);
            assert!(!c.info().bonuses.is_empty(), "{}", c.name());
            assert!(
                !c.info().denied.is_empty() || !c.info().denied_techs.is_empty(),
                "{} is denied nothing",
                c.name()
            );
        }
    }

    #[test]
    fn nothing_a_side_needs_to_start_is_denied() {
        for c in Civ::ALL {
            for k in [
                kinds::VILLAGER,
                kinds::TOWN_CENTER,
                kinds::HOUSE,
                kinds::STOREHOUSE,
                kinds::BARRACKS,
                kinds::FARM,
                kinds::TEMPLE,
                kinds::PRIEST,
            ] {
                assert!(c.allows(k), "{} denied {}", c.name(), kinds::info(k).name);
            }
        }
    }
}
