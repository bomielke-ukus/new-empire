//! Ages and technologies.
//!
//! A technology is researched at a building, costs resources and time, and
//! on completion applies its effects to the owning player's [`Modifiers`].
//! Advancing an age is a technology like any other, researched at the Town
//! Center, with one extra gate: two completed buildings of the current age.
//!
//! This is a static table for the slice, structured as plain data so the
//! `data` crate's RON files can replace it without touching the systems.

use crate::entity::KindId;
use crate::kinds::{self, Cost, Resource};
use serde::{Deserialize, Serialize};

/// A technology id.
pub type TechId = u16;

/// The four ages.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum Age {
    /// Hunter-gatherers.
    #[default]
    Stone = 0,
    /// Farming and the first walls.
    Tool = 1,
    /// Real armies.
    Bronze = 2,
    /// Siege, elephants, wonders.
    Iron = 3,
}

impl Age {
    /// All four, in order.
    pub const ALL: [Age; 4] = [Age::Stone, Age::Tool, Age::Bronze, Age::Iron];

    /// The age after this one, if any.
    pub const fn next(self) -> Option<Age> {
        match self {
            Age::Stone => Some(Age::Tool),
            Age::Tool => Some(Age::Bronze),
            Age::Bronze => Some(Age::Iron),
            Age::Iron => None,
        }
    }

    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            Age::Stone => "Stone Age",
            Age::Tool => "Tool Age",
            Age::Bronze => "Bronze Age",
            Age::Iron => "Iron Age",
        }
    }

    /// Index 0..4.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// From an index; out of range is Iron.
    pub const fn from_index(i: usize) -> Age {
        match i {
            0 => Age::Stone,
            1 => Age::Tool,
            2 => Age::Bronze,
            _ => Age::Iron,
        }
    }
}

/// What a technology does when it completes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Gather rate for one resource, in percent added.
    GatherRate(Resource, i32),
    /// Extra units a villager carries.
    CarryCapacity(i32),
    /// Extra food a farm holds when seeded.
    FarmYield(i32),
    /// Villager walking speed, in percent added.
    VillagerSpeed(i32),
    /// Construction speed, in percent added.
    BuildSpeed(i32),
    /// Move the player to an age.
    AdvanceAge(Age),
}

/// Static properties of a technology.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TechInfo {
    /// The id.
    pub id: TechId,
    /// Display name.
    pub name: &'static str,
    /// Cost.
    pub cost: Cost,
    /// Research time in seconds.
    pub seconds: i32,
    /// Where it is researched.
    pub building: KindId,
    /// The player must be in at least this age. Age advances require the
    /// player to be in exactly the age before.
    pub age: Age,
    /// Technologies that must already be researched.
    pub requires: &'static [TechId],
    /// What it does.
    pub effects: &'static [Effect],
}

impl TechInfo {
    /// Research time in ticks.
    pub const fn ticks(&self) -> u32 {
        (self.seconds * crate::simulation::TICKS_PER_SECOND as i32) as u32
    }

    /// True if this is an age advance.
    pub fn advances_age(&self) -> Option<Age> {
        self.effects.iter().find_map(|e| match e {
            Effect::AdvanceAge(a) => Some(*a),
            _ => None,
        })
    }
}

/// Advance to the Tool Age.
pub const AGE_TOOL: TechId = 1;
/// Advance to the Bronze Age.
pub const AGE_BRONZE: TechId = 2;
/// Advance to the Iron Age.
pub const AGE_IRON: TechId = 3;
/// +15% wood.
pub const WOODWORKING: TechId = 10;
/// +20% stone.
pub const STONE_MINING: TechId = 11;
/// +20% gold.
pub const GOLD_MINING: TechId = 12;
/// +5 carry capacity, +10% villager speed.
pub const CARRYING_BASKETS: TechId = 13;
/// +75 farm food.
pub const DOMESTICATION: TechId = 20;
/// +125 farm food.
pub const PLOUGH: TechId = 21;
/// +20% construction speed.
pub const SCAFFOLDING: TechId = 22;

/// Buildings of the current age needed to advance, excluding houses and the
/// Town Center.
pub const AGE_BUILDINGS_REQUIRED: usize = 2;

const TABLE: &[TechInfo] = &[
    TechInfo {
        id: AGE_TOOL,
        name: "Tool Age",
        cost: [400, 0, 0, 0],
        seconds: 60,
        building: kinds::TOWN_CENTER,
        age: Age::Stone,
        requires: &[],
        effects: &[Effect::AdvanceAge(Age::Tool)],
    },
    TechInfo {
        id: AGE_BRONZE,
        name: "Bronze Age",
        cost: [800, 200, 0, 0],
        seconds: 90,
        building: kinds::TOWN_CENTER,
        age: Age::Tool,
        requires: &[AGE_TOOL],
        effects: &[Effect::AdvanceAge(Age::Bronze)],
    },
    TechInfo {
        id: AGE_IRON,
        name: "Iron Age",
        cost: [1200, 0, 0, 500],
        seconds: 120,
        building: kinds::TOWN_CENTER,
        age: Age::Bronze,
        requires: &[AGE_BRONZE],
        effects: &[Effect::AdvanceAge(Age::Iron)],
    },
    TechInfo {
        id: WOODWORKING,
        name: "Woodworking",
        cost: [60, 30, 0, 0],
        seconds: 40,
        building: kinds::STOREHOUSE,
        age: Age::Tool,
        requires: &[],
        effects: &[Effect::GatherRate(Resource::Wood, 15)],
    },
    TechInfo {
        id: STONE_MINING,
        name: "Stone Mining",
        cost: [60, 0, 20, 0],
        seconds: 40,
        building: kinds::STOREHOUSE,
        age: Age::Tool,
        requires: &[],
        effects: &[Effect::GatherRate(Resource::Stone, 20)],
    },
    TechInfo {
        id: GOLD_MINING,
        name: "Gold Mining",
        cost: [60, 0, 0, 20],
        seconds: 40,
        building: kinds::STOREHOUSE,
        age: Age::Tool,
        requires: &[],
        effects: &[Effect::GatherRate(Resource::Gold, 20)],
    },
    TechInfo {
        id: CARRYING_BASKETS,
        name: "Carrying Baskets",
        cost: [80, 40, 0, 0],
        seconds: 50,
        building: kinds::STOREHOUSE,
        age: Age::Bronze,
        requires: &[WOODWORKING],
        effects: &[Effect::CarryCapacity(5), Effect::VillagerSpeed(10)],
    },
    TechInfo {
        id: DOMESTICATION,
        name: "Domestication",
        cost: [100, 0, 0, 0],
        seconds: 40,
        building: kinds::MARKET,
        age: Age::Tool,
        requires: &[],
        effects: &[Effect::FarmYield(75)],
    },
    TechInfo {
        id: PLOUGH,
        name: "Plough",
        cost: [150, 50, 0, 0],
        seconds: 60,
        building: kinds::MARKET,
        age: Age::Bronze,
        requires: &[DOMESTICATION],
        effects: &[Effect::FarmYield(125)],
    },
    TechInfo {
        id: SCAFFOLDING,
        name: "Scaffolding",
        cost: [50, 80, 0, 0],
        seconds: 40,
        building: kinds::MARKET,
        age: Age::Tool,
        requires: &[],
        effects: &[Effect::BuildSpeed(20)],
    },
];

/// Looks up a technology.
pub fn info(id: TechId) -> Option<&'static TechInfo> {
    TABLE.iter().find(|t| t.id == id)
}

/// Every technology.
pub fn all() -> &'static [TechInfo] {
    TABLE
}

/// Technologies researched at a building kind, in table order.
pub fn at_building(kind: KindId) -> impl Iterator<Item = &'static TechInfo> {
    TABLE.iter().filter(move |t| t.building == kind)
}

/// The technology that advances from `age`, if any.
pub fn age_advance(age: Age) -> Option<&'static TechInfo> {
    TABLE
        .iter()
        .find(|t| t.age == age && t.advances_age().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_chain() {
        assert_eq!(Age::Stone.next(), Some(Age::Tool));
        assert_eq!(Age::Iron.next(), None);
        for a in Age::ALL {
            assert_eq!(Age::from_index(a.index()), a);
        }
        assert!(Age::Stone < Age::Iron);
        assert_eq!(Age::default(), Age::Stone);
    }

    #[test]
    fn table_is_consistent() {
        let mut ids: Vec<_> = TABLE.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TABLE.len(), "ids are unique");
        for t in TABLE {
            assert!(t.seconds > 0 && t.cost.iter().any(|&c| c > 0), "{}", t.name);
            assert!(!t.effects.is_empty(), "{}", t.name);
            assert!(
                kinds::info(t.building).buildable || t.building == kinds::TOWN_CENTER,
                "{}: researched somewhere real",
                t.name
            );
            for r in t.requires {
                assert!(info(*r).is_some(), "{}: requires unknown tech {r}", t.name);
                assert!(
                    info(*r).unwrap().age <= t.age,
                    "{}: prerequisite from a later age",
                    t.name
                );
            }
            if let Some(a) = t.advances_age() {
                assert_eq!(
                    t.age.next(),
                    Some(a),
                    "{}: advances to the next age",
                    t.name
                );
            }
        }
        assert_eq!(age_advance(Age::Stone).map(|t| t.id), Some(AGE_TOOL));
        assert_eq!(age_advance(Age::Bronze).map(|t| t.id), Some(AGE_IRON));
        assert!(age_advance(Age::Iron).is_none());
        assert_eq!(at_building(kinds::STOREHOUSE).count(), 4);
        assert_eq!(info(AGE_TOOL).unwrap().ticks(), 1200);
        assert!(info(999).is_none());
    }
}
