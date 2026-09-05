//! The static table of entity kinds.
//!
//! This is a placeholder for the data-driven roster that arrives with the
//! `data` crate in M3. Until then the handful of kinds the world needs are
//! defined here so the simulation, the generator and the renderer agree on
//! ids, footprints and hit points.

use crate::entity::KindId;
use crate::fx::Fx;

/// Villager: gathers, builds, repairs.
pub const VILLAGER: KindId = 1;
/// Scout: fast, wide vision, weak.
pub const SCOUT: KindId = 2;
/// Town Center: the settlement's heart.
pub const TOWN_CENTER: KindId = 10;
/// House: population.
pub const HOUSE: KindId = 11;
/// A tree. Removed when its wood is exhausted.
pub const TREE: KindId = 100;
/// A berry bush.
pub const BERRY_BUSH: KindId = 101;
/// A gold vein.
pub const GOLD_MINE: KindId = 102;
/// A stone vein.
pub const STONE_MINE: KindId = 103;
/// A gazelle: huntable food that runs.
pub const GAZELLE: KindId = 110;

/// Owner id for things nobody owns: trees, mines, animals.
pub const GAIA: u8 = 255;

/// What a resource node yields.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resource {
    /// Food.
    Food,
    /// Wood.
    Wood,
    /// Stone.
    Stone,
    /// Gold.
    Gold,
}

/// Static properties of a kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KindInfo {
    /// The id this describes.
    pub id: KindId,
    /// Display name.
    pub name: &'static str,
    /// Side length of the tile footprint; 0 for mobile units, which never
    /// block a tile.
    pub footprint: u8,
    /// Hit points at full health.
    pub max_health: i32,
    /// Whether it can move.
    pub mobile: bool,
    /// Movement speed in tiles per second (mobile kinds only).
    pub speed_per_second: Fx,
    /// Resource carried, with the amount a fresh node holds.
    pub resource: Option<(Resource, i32)>,
}

const fn unit(id: KindId, name: &'static str, hp: i32, speed_tenths: i32) -> KindInfo {
    KindInfo {
        id,
        name,
        footprint: 0,
        max_health: hp,
        mobile: true,
        speed_per_second: Fx::from_ratio(speed_tenths, 10),
        resource: None,
    }
}

const fn building(id: KindId, name: &'static str, hp: i32, footprint: u8) -> KindInfo {
    KindInfo {
        id,
        name,
        footprint,
        max_health: hp,
        mobile: false,
        speed_per_second: Fx::ZERO,
        resource: None,
    }
}

const fn node(id: KindId, name: &'static str, hp: i32, r: Resource, amount: i32) -> KindInfo {
    KindInfo {
        id,
        name,
        footprint: 1,
        max_health: hp,
        mobile: false,
        speed_per_second: Fx::ZERO,
        resource: Some((r, amount)),
    }
}

const TABLE: &[KindInfo] = &[
    unit(VILLAGER, "Villager", 25, 9),
    unit(SCOUT, "Scout", 45, 16),
    building(TOWN_CENTER, "Town Center", 600, 3),
    building(HOUSE, "House", 75, 2),
    node(TREE, "Tree", 20, Resource::Wood, 75),
    node(BERRY_BUSH, "Berry Bush", 1, Resource::Food, 150),
    node(GOLD_MINE, "Gold Vein", 1, Resource::Gold, 400),
    node(STONE_MINE, "Stone Vein", 1, Resource::Stone, 350),
    KindInfo {
        id: GAZELLE,
        name: "Gazelle",
        footprint: 0,
        max_health: 8,
        mobile: true,
        speed_per_second: Fx::from_ratio(14, 10),
        resource: Some((Resource::Food, 140)),
    },
];

const UNKNOWN: KindInfo = unit(0, "Unknown", 1, 10);

/// Looks up a kind. Unknown ids get a harmless default rather than a panic,
/// because a replay from a newer build must not crash an older one outright.
pub fn info(kind: KindId) -> &'static KindInfo {
    TABLE.iter().find(|k| k.id == kind).unwrap_or(&UNKNOWN)
}

/// Every known kind.
pub fn all() -> &'static [KindInfo] {
    TABLE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_consistent() {
        for k in all() {
            assert_eq!(info(k.id).id, k.id);
            assert_eq!(
                k.mobile,
                k.footprint == 0,
                "{}: mobile kinds have no footprint",
                k.name
            );
            assert_eq!(
                k.mobile,
                k.speed_per_second.is_positive(),
                "{}: mobile kinds move",
                k.name
            );
            assert!(k.max_health > 0);
        }
        assert_eq!(info(9999).name, "Unknown");
        assert_eq!(info(TOWN_CENTER).footprint, 3);
        assert_eq!(info(TREE).resource, Some((Resource::Wood, 75)));
        assert!(info(GAZELLE).mobile && info(GAZELLE).resource.is_some());
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = all().iter().map(|k| k.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all().len());
    }
}
