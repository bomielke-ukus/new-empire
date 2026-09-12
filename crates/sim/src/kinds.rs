//! The static table of entity kinds.
//!
//! This is a placeholder for the data-driven roster that arrives with the
//! `data` crate in M3. Until then the handful of kinds the world needs are
//! defined here so the simulation, the generator and the renderer agree on
//! ids, footprints, costs and hit points.

use crate::entity::KindId;
use crate::fx::Fx;
use crate::tech::Age;

/// Villager: gathers, builds, repairs.
pub const VILLAGER: KindId = 1;
/// Scout: fast, wide vision, weak.
pub const SCOUT: KindId = 2;
/// Clubman: the first soldier.
pub const CLUBMAN: KindId = 3;
/// Axeman: the Clubman line after the Axe upgrade.
pub const AXEMAN: KindId = 4;
/// Spearman: the anti-cavalry infantry.
pub const SPEARMAN: KindId = 5;
/// Slinger: the cheap anti-infantry ranged unit.
pub const SLINGER: KindId = 6;
/// Bowman: the backbone ranged unit.
pub const BOWMAN: KindId = 7;
/// Light Cavalry: fast, raids villagers.
pub const LIGHT_CAVALRY: KindId = 8;
/// Town Center: the settlement's heart.
pub const TOWN_CENTER: KindId = 10;
/// House: population.
pub const HOUSE: KindId = 11;
/// Storehouse: universal drop-off.
pub const STOREHOUSE: KindId = 12;
/// Barracks: infantry.
pub const BARRACKS: KindId = 13;
/// Farm: renewable food, reseeded for wood.
pub const FARM: KindId = 14;
/// Archery Range: ranged units.
pub const ARCHERY_RANGE: KindId = 15;
/// Stable: mounted units.
pub const STABLE: KindId = 16;
/// Market: economy technology and trade.
pub const MARKET: KindId = 17;
/// Watch Tower: static defence.
pub const WATCH_TOWER: KindId = 18;
/// Temple: priests.
pub const TEMPLE: KindId = 19;
/// Academy: heavy infantry.
pub const ACADEMY: KindId = 20;
/// Siege Workshop.
pub const SIEGE_WORKSHOP: KindId = 21;
/// Government Centre: civic upgrades.
pub const GOVERNMENT_CENTRE: KindId = 22;

/// Wood to reseed a farm.
pub const FARM_RESEED_COST: Cost = [0, 60, 0, 0];
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Resource {
    /// Food.
    Food = 0,
    /// Wood.
    Wood = 1,
    /// Stone.
    Stone = 2,
    /// Gold.
    Gold = 3,
}

impl Resource {
    /// All four, in stockpile order.
    pub const ALL: [Resource; 4] = [
        Resource::Food,
        Resource::Wood,
        Resource::Stone,
        Resource::Gold,
    ];

    /// Index into a `[i32; 4]` stockpile.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// From a stockpile index; out of range is food.
    pub const fn from_index(i: usize) -> Resource {
        match i {
            1 => Resource::Wood,
            2 => Resource::Stone,
            3 => Resource::Gold,
            _ => Resource::Food,
        }
    }

    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            Resource::Food => "Food",
            Resource::Wood => "Wood",
            Resource::Stone => "Stone",
            Resource::Gold => "Gold",
        }
    }

    /// Villager gather rate for this resource, in units per second. One base
    /// rate for all four (`docs/02` §3.3); civilisation bonuses and
    /// technologies modify it per player.
    pub const fn gather_rate(self) -> Fx {
        BASE_GATHER_RATE
    }
}

/// A cost in `[food, wood, stone, gold]`.
pub type Cost = [i32; 4];

/// What a kind is for the counter system (`docs/02` §8): bonus damage and
/// technology bonuses apply per class.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Class {
    /// Trees, mines, bushes: not a combatant at all.
    #[default]
    Other = 0,
    /// Villagers.
    Villager = 1,
    /// Foot soldiers with a melee weapon.
    Infantry = 2,
    /// Foot soldiers with a ranged weapon.
    Ranged = 3,
    /// Mounted units.
    Cavalry = 4,
    /// Siege engines.
    Siege = 5,
    /// Buildings.
    Building = 6,
    /// Huntable animals.
    Animal = 7,
}

impl Class {
    /// Every class, in index order.
    pub const ALL: [Class; 8] = [
        Class::Other,
        Class::Villager,
        Class::Infantry,
        Class::Ranged,
        Class::Cavalry,
        Class::Siege,
        Class::Building,
        Class::Animal,
    ];

    /// Index into a per-class table.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Display name, plural, as a tooltip says it.
    pub const fn plural(self) -> &'static str {
        match self {
            Class::Other => "things",
            Class::Villager => "villagers",
            Class::Infantry => "infantry",
            Class::Ranged => "archers",
            Class::Cavalry => "cavalry",
            Class::Siege => "siege",
            Class::Building => "buildings",
            Class::Animal => "animals",
        }
    }
}

/// How an attack is delivered, and which armour resists it
/// (`docs/02` §8, `GD-COMBAT-01`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum DamageType {
    /// Resisted by melee armour.
    #[default]
    Melee,
    /// Resisted by pierce armour.
    Pierce,
    /// Resisted by nothing, and hits friends in the blast (`GD-COMBAT-04`).
    Siege,
}

impl DamageType {
    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            DamageType::Melee => "melee",
            DamageType::Pierce => "pierce",
            DamageType::Siege => "siege",
        }
    }
}

/// A kind's fighting numbers. Zero attack means it cannot fight.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Combat {
    /// Damage per hit before armour.
    pub attack: i32,
    /// What kind of hit.
    pub damage: DamageType,
    /// Reach in tiles; 0 is hand to hand (an adjacent tile).
    pub range: i32,
    /// Ticks between hits.
    pub reload_ticks: u32,
    /// Armour against melee hits.
    pub melee_armour: i32,
    /// Armour against pierce hits.
    pub pierce_armour: i32,
    /// Extra damage against a class, per hit (`GD-COMBAT-02`).
    pub bonuses: &'static [(Class, i32)],
    /// How far it sees, in tiles.
    pub line_of_sight: i32,
}

/// Ticks between hits for most units: a hit and a half a second.
pub const RELOAD_TICKS: u32 = 30;

const NO_COMBAT: Combat = Combat {
    attack: 0,
    damage: DamageType::Melee,
    range: 0,
    reload_ticks: RELOAD_TICKS,
    melee_armour: 0,
    pierce_armour: 0,
    bonuses: &[],
    line_of_sight: 4,
};

const fn melee(attack: i32, melee_armour: i32, pierce_armour: i32) -> Combat {
    Combat {
        attack,
        melee_armour,
        pierce_armour,
        ..NO_COMBAT
    }
}

const fn ranged(attack: i32, range: i32, pierce_armour: i32) -> Combat {
    Combat {
        attack,
        damage: DamageType::Pierce,
        range,
        pierce_armour,
        line_of_sight: range + 1,
        ..NO_COMBAT
    }
}

/// The base gather rate, resources per second.
pub const BASE_GATHER_RATE: Fx = Fx::from_ratio(45, 100);

/// How much a villager carries before walking it home.
pub const CARRY_CAPACITY: i32 = 10;
/// Villagers that may work on one site at once; more do nothing.
pub const MAX_BUILDERS: usize = 5;

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
    /// What it costs to build or train.
    pub cost: Cost,
    /// Seconds to construct (buildings) or train (units).
    pub build_seconds: i32,
    /// Population supported once complete.
    pub pop_provided: u32,
    /// Population consumed while alive.
    pub pop_cost: u32,
    /// Accepts gathered resources.
    pub dropoff: bool,
    /// Trains villagers.
    pub trains: bool,
    /// A player may build it.
    pub buildable: bool,
    /// The age from which it is available.
    pub age: Age,
    /// What it is to the counter system.
    pub class: Class,
    /// How it fights.
    pub combat: Combat,
    /// The building that trains it, for units.
    pub trained_at: Option<KindId>,
}

/// Units of construction work per builder per tick at normal speed.
///
/// A site's `construction` counter is in these units, not ticks, so a
/// percentage build-speed bonus lands exactly instead of rounding away.
pub const BUILD_WORK_PER_TICK: u32 = 100;

impl KindInfo {
    /// Construction or training time in ticks.
    pub const fn build_ticks(&self) -> u32 {
        (self.build_seconds * crate::simulation::TICKS_PER_SECOND as i32) as u32
    }

    /// Construction work to finish a site, in [`BUILD_WORK_PER_TICK`] units.
    pub const fn build_work(&self) -> u32 {
        self.build_ticks() * BUILD_WORK_PER_TICK
    }
}

const BASE: KindInfo = KindInfo {
    id: 0,
    name: "Unknown",
    footprint: 0,
    max_health: 1,
    mobile: false,
    speed_per_second: Fx::ZERO,
    resource: None,
    cost: [0; 4],
    build_seconds: 0,
    pop_provided: 0,
    pop_cost: 0,
    dropoff: false,
    trains: false,
    buildable: false,
    age: Age::Stone,
    class: Class::Other,
    combat: NO_COMBAT,
    trained_at: None,
};

const fn unit(
    id: KindId,
    name: &'static str,
    hp: i32,
    speed_tenths: i32,
    cost: Cost,
    train_seconds: i32,
) -> KindInfo {
    KindInfo {
        id,
        name,
        max_health: hp,
        mobile: true,
        speed_per_second: Fx::from_ratio(speed_tenths, 10),
        cost,
        build_seconds: train_seconds,
        pop_cost: 1,
        ..BASE
    }
}

const fn building(
    id: KindId,
    name: &'static str,
    hp: i32,
    footprint: u8,
    cost: Cost,
    seconds: i32,
) -> KindInfo {
    KindInfo {
        id,
        name,
        footprint,
        max_health: hp,
        cost,
        build_seconds: seconds,
        buildable: true,
        class: Class::Building,
        ..BASE
    }
}

const fn node(id: KindId, name: &'static str, hp: i32, r: Resource, amount: i32) -> KindInfo {
    KindInfo {
        id,
        name,
        footprint: 1,
        max_health: hp,
        resource: Some((r, amount)),
        ..BASE
    }
}

const TABLE: &[KindInfo] = &[
    KindInfo {
        class: Class::Villager,
        // Fights badly (`docs/02` §5.1).
        combat: melee(3, 0, 0),
        trained_at: Some(TOWN_CENTER),
        ..unit(VILLAGER, "Villager", 25, 9, [50, 0, 0, 0], 25)
    },
    KindInfo {
        class: Class::Cavalry,
        // Weak attack, wide line of sight.
        combat: Combat {
            line_of_sight: 8,
            ..melee(2, 0, 0)
        },
        trained_at: Some(STABLE),
        ..unit(SCOUT, "Scout", 45, 16, [60, 0, 0, 0], 30)
    },
    KindInfo {
        age: Age::Stone,
        class: Class::Infantry,
        combat: melee(3, 0, 0),
        trained_at: Some(BARRACKS),
        ..unit(CLUBMAN, "Clubman", 40, 11, [50, 0, 0, 0], 26)
    },
    KindInfo {
        age: Age::Tool,
        class: Class::Infantry,
        combat: melee(5, 0, 0),
        trained_at: Some(BARRACKS),
        ..unit(AXEMAN, "Axeman", 50, 11, [50, 20, 0, 0], 26)
    },
    KindInfo {
        age: Age::Tool,
        class: Class::Infantry,
        combat: Combat {
            bonuses: &[(Class::Cavalry, 6)],
            ..melee(4, 0, 1)
        },
        trained_at: Some(BARRACKS),
        ..unit(SPEARMAN, "Spearman", 45, 11, [40, 20, 0, 0], 26)
    },
    KindInfo {
        age: Age::Tool,
        class: Class::Ranged,
        combat: Combat {
            bonuses: &[(Class::Infantry, 4)],
            ..ranged(4, 4, 0)
        },
        trained_at: Some(ARCHERY_RANGE),
        ..unit(SLINGER, "Slinger", 40, 10, [40, 0, 10, 0], 25)
    },
    KindInfo {
        age: Age::Tool,
        class: Class::Ranged,
        combat: ranged(5, 5, 0),
        trained_at: Some(ARCHERY_RANGE),
        ..unit(BOWMAN, "Bowman", 40, 10, [40, 20, 0, 0], 30)
    },
    KindInfo {
        age: Age::Tool,
        class: Class::Cavalry,
        combat: Combat {
            line_of_sight: 6,
            ..melee(7, 0, 0)
        },
        trained_at: Some(STABLE),
        ..unit(LIGHT_CAVALRY, "Light Cavalry", 90, 20, [60, 0, 0, 20], 40)
    },
    KindInfo {
        pop_provided: 5,
        dropoff: true,
        trains: true,
        ..building(TOWN_CENTER, "Town Center", 600, 3, [0, 200, 0, 0], 120)
    },
    KindInfo {
        pop_provided: 5,
        ..building(HOUSE, "House", 75, 2, [0, 30, 0, 0], 25)
    },
    KindInfo {
        dropoff: true,
        ..building(STOREHOUSE, "Storehouse", 200, 2, [0, 100, 0, 0], 30)
    },
    KindInfo {
        trains: true,
        ..building(BARRACKS, "Barracks", 350, 2, [0, 125, 0, 0], 45)
    },
    KindInfo {
        age: Age::Tool,
        resource: Some((Resource::Food, 250)),
        ..building(FARM, "Farm", 60, 2, [0, 75, 0, 0], 20)
    },
    KindInfo {
        age: Age::Tool,
        trains: true,
        ..building(ARCHERY_RANGE, "Archery Range", 350, 2, [0, 150, 0, 0], 45)
    },
    KindInfo {
        age: Age::Tool,
        trains: true,
        ..building(STABLE, "Stable", 350, 2, [0, 150, 0, 0], 45)
    },
    KindInfo {
        age: Age::Tool,
        ..building(MARKET, "Market", 300, 2, [0, 150, 0, 0], 45)
    },
    KindInfo {
        age: Age::Tool,
        ..building(WATCH_TOWER, "Watch Tower", 250, 1, [0, 0, 120, 0], 40)
    },
    KindInfo {
        age: Age::Bronze,
        ..building(TEMPLE, "Temple", 400, 2, [0, 200, 0, 0], 60)
    },
    KindInfo {
        age: Age::Bronze,
        ..building(ACADEMY, "Academy", 400, 2, [0, 200, 0, 0], 60)
    },
    KindInfo {
        age: Age::Bronze,
        ..building(SIEGE_WORKSHOP, "Siege Workshop", 400, 2, [0, 200, 0, 0], 60)
    },
    KindInfo {
        age: Age::Bronze,
        ..building(
            GOVERNMENT_CENTRE,
            "Government Centre",
            400,
            2,
            [0, 175, 0, 0],
            60,
        )
    },
    node(TREE, "Tree", 20, Resource::Wood, 75),
    node(BERRY_BUSH, "Berry Bush", 1, Resource::Food, 150),
    node(GOLD_MINE, "Gold Vein", 1, Resource::Gold, 400),
    node(STONE_MINE, "Stone Vein", 1, Resource::Stone, 350),
    KindInfo {
        id: GAZELLE,
        name: "Gazelle",
        max_health: 8,
        class: Class::Animal,
        mobile: true,
        speed_per_second: Fx::from_ratio(14, 10),
        resource: Some((Resource::Food, 140)),
        ..BASE
    },
];

const UNKNOWN: KindInfo = KindInfo {
    mobile: true,
    speed_per_second: Fx::ONE,
    ..BASE
};

/// Looks up a kind. Unknown ids get a harmless default rather than a panic,
/// because a replay from a newer build must not crash an older one outright.
pub fn info(kind: KindId) -> &'static KindInfo {
    TABLE.iter().find(|k| k.id == kind).unwrap_or(&UNKNOWN)
}

/// Every known kind.
pub fn all() -> &'static [KindInfo] {
    TABLE
}

/// The units a building kind trains, in table order. Whether a player may
/// train one right now (age, line upgrades) is the simulation's question.
pub fn trained_at(building: KindId) -> impl Iterator<Item = &'static KindInfo> {
    TABLE.iter().filter(move |k| k.trained_at == Some(building))
}

/// True if a villager can gather from this kind: a static node with a
/// resource. Animals need hunting, which arrives with combat. Farms count,
/// but only their owner may work them; the simulation checks that.
pub fn gatherable(kind: KindId) -> bool {
    let k = info(kind);
    k.resource.is_some() && !k.mobile
}

/// Buildings that count toward advancing an age (`docs/02` §4): what a
/// player builds, less the Town Center and Houses the table excludes, and
/// less Farms, which are fields rather than a commitment to a direction.
pub fn counts_for_age(kind: KindId) -> bool {
    let k = info(kind);
    k.buildable && !k.mobile && kind != HOUSE && kind != TOWN_CENTER && kind != FARM
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
            if k.buildable {
                assert!(
                    k.build_seconds > 0 && k.cost.iter().any(|&c| c > 0),
                    "{}",
                    k.name
                );
            }
        }
        assert_eq!(info(9999).name, "Unknown");
        assert_eq!(info(TOWN_CENTER).footprint, 3);
        assert_eq!(info(TREE).resource, Some((Resource::Wood, 75)));
        assert!(info(GAZELLE).mobile && info(GAZELLE).resource.is_some());
        assert!(info(TOWN_CENTER).dropoff && info(STOREHOUSE).dropoff && !info(HOUSE).dropoff);
        assert_eq!(info(HOUSE).cost, [0, 30, 0, 0]);
        assert_eq!(info(VILLAGER).build_ticks(), 25 * 20);
        assert_eq!(info(VILLAGER).pop_cost, 1);
        assert_eq!(info(HOUSE).pop_provided, 5);
        assert!(info(HOUSE).buildable && !info(TREE).buildable && !info(VILLAGER).buildable);
        for k in all() {
            if let Some(b) = k.trained_at {
                assert!(
                    k.mobile && info(b).trains,
                    "{}: trained at a trainer",
                    k.name
                );
                assert!(
                    k.build_seconds > 0 && k.cost.iter().any(|&c| c > 0),
                    "{}",
                    k.name
                );
            }
            if k.combat.attack > 0 {
                assert!(k.combat.reload_ticks > 0, "{}", k.name);
                assert!(k.class != Class::Other, "{}: a fighter has a class", k.name);
            }
            for (c, b) in k.combat.bonuses {
                assert!(*b > 0 && *c != Class::Other, "{}: bonus vs {c:?}", k.name);
            }
        }
        assert_eq!(trained_at(BARRACKS).count(), 3);
        assert_eq!(trained_at(ARCHERY_RANGE).count(), 2);
        assert_eq!(trained_at(STABLE).count(), 2);
        assert_eq!(trained_at(TOWN_CENTER).count(), 1);
        assert_eq!(info(SPEARMAN).combat.bonuses, &[(Class::Cavalry, 6)]);
        assert_eq!(info(BOWMAN).combat.damage, DamageType::Pierce);
        assert_eq!(info(TREE).combat.attack, 0);
        for c in Class::ALL {
            assert_eq!(Class::ALL[c.index()], c);
        }
        assert!(
            gatherable(TREE) && gatherable(GOLD_MINE) && !gatherable(GAZELLE) && !gatherable(HOUSE)
        );
        for r in Resource::ALL {
            assert_eq!(Resource::from_index(r.index()), r);
            assert!(r.gather_rate().is_positive());
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = all().iter().map(|k| k.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all().len());
    }
}
