//! The opponent's technologies (`docs/02` §7): from the Tool Age, the
//! improvements a player researches at the Storehouse, the Market, the
//! Barracks and the Archery Range, each once it is worth having to this
//! side, in the order a player would want them. The ages are the
//! economy's and the line upgrades the military's; this is the rest.
//!
//! It spends only what is over the cost by a reserve, and never what the
//! economy is saving for the next age. Until the army has gone out once
//! it gets what the army leaves, so the first attack is not late; from
//! then it spends before the army (`crate::Opponent::think`). Measured
//! over 80 matches of Hard against Hard with the sides swapped, a side
//! that researches wins as often as one that does not: what it spends
//! on technology it does not spend on soldiers, and the two come out
//! even. It researches because a player does, and its later soldiers are
//! the stronger for it.

use fogged::kinds::{self, Class, Cost, Resource};
use fogged::tech::{self, TechId};
use fogged::{CommandKind, FoggedView, Job, ResearchError, Sighting};

use crate::economy::{afford, spend, BuildOrder};

/// What makes a technology worth having to a side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Worth {
    /// Always: every side gathers wood and builds.
    Always,
    /// A farm of the side's stands.
    Farms,
    /// The side's villagers gather this.
    Gathering(Resource),
    /// The side has soldiers of this class.
    Soldiers(Class),
    /// The side has soldiers of either class.
    EitherSoldiers(Class, Class),
}

/// The technologies the opponent researches, most wanted first: the wood
/// everything is built of, the army's blows, the farms the food comes
/// from once the bushes are gone, the gold the later ages' soldiers cost,
/// then armour and arrows for what the army is made of, then the rest.
pub const WANTED: [(TechId, Worth); 10] = [
    (tech::WOODWORKING, Worth::Always),
    (
        tech::TOOLWORKING,
        Worth::EitherSoldiers(Class::Infantry, Class::Cavalry),
    ),
    (tech::DOMESTICATION, Worth::Farms),
    (tech::GOLD_MINING, Worth::Gathering(Resource::Gold)),
    (tech::LEATHER_ARMOUR, Worth::Soldiers(Class::Infantry)),
    (tech::FLETCHING, Worth::Soldiers(Class::Ranged)),
    (tech::CARRYING_BASKETS, Worth::Always),
    (tech::PLOUGH, Worth::Farms),
    (tech::SCAFFOLDING, Worth::Always),
    (tech::STONE_MINING, Worth::Gathering(Resource::Stone)),
];

/// Kept over a technology's cost, of each resource it costs: food for the
/// next villager and soldier, wood for the next house, gold and stone for
/// what the army is paid in.
const RESERVE: Cost = [100, 50, 50, 50];

/// The cost with the reserve on each resource it costs.
fn reserved(cost: Cost) -> Cost {
    let mut out = cost;
    for (c, r) in out.iter_mut().zip(RESERVE) {
        if *c > 0 {
            *c += r;
        }
    }
    out
}

/// Whether a technology is worth having to the side whose things are
/// `mine`.
fn worth(w: Worth, mine: &[&Sighting]) -> bool {
    let soldiers = |class: Class| {
        mine.iter().any(|s| {
            let k = kinds::info(s.kind);
            k.mobile && k.class == class && k.combat.attack > 0
        })
    };
    match w {
        Worth::Always => true,
        Worth::Farms => mine.iter().any(|s| s.kind == kinds::FARM && !s.site),
        Worth::Gathering(r) => mine.iter().any(|s| s.job == Job::Gathering(r)),
        Worth::Soldiers(c) => soldiers(c),
        Worth::EitherSoldiers(a, b) => soldiers(a) || soldiers(b),
    }
}

/// One thought: at most one technology begun, the most wanted of those
/// the side can research now. If that one costs more than `stock` holds
/// over the reserve it waits for it, and nothing less wanted goes first.
pub fn think(view: &FoggedView<'_>, order: &BuildOrder, stock: &mut Cost) -> Vec<CommandKind> {
    let mut out = Vec::new();
    if !order.research {
        return out;
    }
    let seen = view.sightings();
    let mine: Vec<&Sighting> = seen.iter().filter(|s| s.owner == view.player()).collect();
    for (id, w) in WANTED {
        let Some(t) = tech::info(id) else {
            continue;
        };
        if !worth(w, &mine) {
            continue;
        }
        // The first of the side's buildings it can be researched at;
        // unaffordable counts, so it is waited for.
        let mut at = None;
        for b in mine.iter().filter(|s| s.kind == t.building && !s.site) {
            match view.can_research(b.id, id) {
                Ok(()) | Err(ResearchError::Unaffordable) => {
                    at = Some(b.id);
                    break;
                }
                Err(_) => {}
            }
        }
        let Some(building) = at else {
            continue;
        };
        if afford(stock, &reserved(t.cost)) {
            out.push(CommandKind::Research { building, tech: id });
            spend(stock, &t.cost);
        }
        break;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_wanted_technology_is_one_the_rest_of_the_opponent_leaves() {
        for (id, _) in WANTED {
            let t = tech::info(id).expect("a real technology");
            assert!(t.advances_age().is_none(), "{}: the economy's", t.name);
            assert!(t.upgrades_line().is_none(), "{}: the military's", t.name);
        }
        // And every technology that is neither is wanted.
        for t in tech::all() {
            if t.advances_age().is_none() && t.upgrades_line().is_none() {
                assert!(
                    WANTED.iter().any(|(id, _)| *id == t.id),
                    "{} is never researched",
                    t.name
                );
            }
        }
    }

    #[test]
    fn a_prerequisite_is_wanted_before_what_needs_it() {
        let place = |id: TechId| WANTED.iter().position(|(t, _)| *t == id).unwrap();
        for (id, _) in WANTED {
            for need in tech::info(id).unwrap().requires {
                if tech::info(*need).unwrap().advances_age().is_some() {
                    continue;
                }
                assert!(place(*need) < place(id));
            }
        }
    }
}
