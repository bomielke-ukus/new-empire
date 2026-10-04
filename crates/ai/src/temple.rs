//! The Temple's priests (`docs/02` §5.5, §10): from the Bronze Age, once
//! the economy has built a Temple by the Town Center, a few priests who
//! fetch every relic the
//! side knows of into it (`GD-WIN-03`), and otherwise go out with the army
//! and turn what they can of the enemy's (`GD-PRIEST-01`). Like everything
//! the opponent does, it works from the view: a relic it has not seen is a
//! relic it does not go for.

use fogged::kinds::{self, Class, Cost};
use fogged::{Age, CommandKind, EntityId, FoggedView, Item, Job, Sighting, Vec2Fx};

use crate::economy::{afford, spend, BuildOrder};

/// Gold kept before a priest is paid for.
const PRIEST_RESERVE: Cost = [100, 0, 0, 50];
/// How far a priest looks for a unit to turn, in tiles: its sight.
const CONVERT_LOOK: i32 = 8;
/// How far from the Town Center a priest still counts as at home.
const HOME_RADIUS: i32 = 20;

/// One thought: the commands for the Temple and its priests, with
/// `stock` being what the economy and the army have not spent.
pub fn think(
    view: &FoggedView<'_>,
    order: &BuildOrder,
    stock: &mut Cost,
    army_out: Option<Vec2Fx>,
) -> Vec<CommandKind> {
    let mut out = Vec::new();
    let Some(me) = view.me() else {
        return out;
    };
    if order.priests == 0 || me.age.index() < Age::Bronze.index() {
        return out;
    }
    let seen = view.sightings();
    let mine: Vec<&Sighting> = seen.iter().filter(|s| s.owner == view.player()).collect();
    let Some(tc) = mine
        .iter()
        .find(|s| s.kind == kinds::TOWN_CENTER && !s.site)
        .copied()
    else {
        return out;
    };
    let tc_tile = (tc.pos.x.floor(), tc.pos.y.floor());
    let temples: Vec<&Sighting> = mine
        .iter()
        .filter(|s| s.kind == kinds::TEMPLE)
        .copied()
        .collect();

    // The Temple is the economy's to build, with the age's buildings.
    let Some(temple) = temples.iter().find(|s| !s.site).copied() else {
        return out;
    };

    // ----- Priests, to the order's number.
    let priests: Vec<&Sighting> = mine
        .iter()
        .filter(|s| s.kind == kinds::PRIEST)
        .copied()
        .collect();
    let queued = view
        .queue(temple.id)
        .iter()
        .filter(|i| matches!(i, Item::Unit(k) if *k == kinds::PRIEST))
        .count();
    if priests.len() + queued < order.priests as usize
        && view.queue(temple.id).len() < 2
        && view.can_train(temple.id, kinds::PRIEST).is_ok()
    {
        let cost = kinds::info(kinds::PRIEST).cost;
        let mut with_reserve = cost;
        for (c, r) in with_reserve.iter_mut().zip(PRIEST_RESERVE) {
            *c += r;
        }
        if afford(stock, &with_reserve) {
            out.push(CommandKind::Train {
                building: temple.id,
                kind: kinds::PRIEST,
            });
            spend(stock, &cost);
        }
    }

    // ----- The relics it knows of, lying on the ground: in sight now, or
    // remembered where they were seen. Each goes to one priest.
    let mut relics: Vec<(EntityId, Vec2Fx)> = seen
        .iter()
        .filter(|s| s.kind == kinds::RELIC && s.owner == kinds::GAIA)
        .map(|s| (s.id, s.pos))
        .collect();
    for r in view.remembered() {
        if r.kind == kinds::RELIC && !relics.iter().any(|(id, _)| *id == r.id) {
            relics.push((r.id, fogged::nav::centre(r.tile)));
        }
    }
    let taken: Vec<EntityId> = priests
        .iter()
        .filter_map(|p| match p.job {
            Job::Fetching(r) => Some(r),
            _ => None,
        })
        .collect();
    relics.retain(|(id, _)| !taken.contains(id));

    let enemy = |owner: u8| owner != view.player() && owner != kinds::GAIA;
    let home = |p: Vec2Fx| {
        (p.x.floor() - tc_tile.0).abs() <= HOME_RADIUS
            && (p.y.floor() - tc_tile.1).abs() <= HOME_RADIUS
    };
    for p in priests.iter().filter(|p| p.job == Job::Idle && !p.inside) {
        // A relic in hand goes into the Temple.
        if view.carrying_relic(p.id) {
            out.push(CommandKind::Relic {
                ids: vec![p.id],
                target: temple.id,
            });
            continue;
        }
        // The nearest relic nobody is fetching.
        if let Some(k) = (0..relics.len()).min_by_key(|&k| {
            let (id, pos) = relics[k];
            (p.pos.distance_sq_raw(pos), id.index())
        }) {
            let (relic, _) = relics.remove(k);
            out.push(CommandKind::Relic {
                ids: vec![p.id],
                target: relic,
            });
            continue;
        }
        // With faith, the dearest enemy unit in sight.
        if view.faith_full(p.id) {
            let look = fogged::Fx::from_int(CONVERT_LOOK);
            let limit = look.raw() as u64 * look.raw() as u64;
            let worth = |s: &Sighting| kinds::info(s.kind).cost.iter().sum::<i32>();
            let target = seen
                .iter()
                .filter(|s| {
                    enemy(s.owner)
                        && kinds::info(s.kind).mobile
                        && kinds::info(s.kind).class != Class::Animal
                        && p.pos.distance_sq_raw(s.pos) <= limit
                })
                .max_by_key(|s| (worth(s), std::cmp::Reverse(s.id.index())));
            if let Some(t) = target {
                out.push(CommandKind::Attack {
                    ids: vec![p.id],
                    target: t.id,
                });
                continue;
            }
        }
        // The army is out: go with it, a little behind.
        if let Some(to) = army_out {
            if home(p.pos) {
                out.push(CommandKind::Move {
                    ids: vec![p.id],
                    target: to,
                });
            }
        }
    }
    out
}
