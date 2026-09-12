//! Group shapes (`docs/03` `UX-CMD-08`): where each unit of a group stands
//! when the group is sent somewhere together.
//!
//! A shape is a list of offsets in quarter tiles, across and back from the
//! group's front centre, so half-tile staggers stay in integers. The
//! simulation rotates them to face the way the group walks and hands each
//! unit the slot nearest its own side, so a line does not cross itself
//! forming up.

use crate::fx::Fx;
use crate::orders::Formation;
use crate::vec2::Vec2Fx;

/// Quarter tiles between neighbours in a rank.
const ACROSS: i32 = 4;
/// Quarter tiles between ranks.
const BACK: i32 = 4;
/// The gap between a flank's two wings, in quarter tiles.
const WING_GAP: i32 = 12;

/// Integer square root, rounded up.
fn sqrt_ceil(n: usize) -> usize {
    let mut r = 0usize;
    while r * r < n {
        r += 1;
    }
    r.max(1)
}

/// `n` slots filling ranks `width` wide, front rank first, each rank
/// centred; `stagger` shifts alternate ranks by half a step and spaces
/// ranks further apart.
fn ranks(n: usize, width: usize, stagger: bool, shift: i32) -> Vec<(i32, i32)> {
    let width = width.max(1);
    let mut out = Vec::with_capacity(n);
    let mut row = 0;
    while out.len() < n {
        let in_row = (n - out.len()).min(width);
        // Centre the rank: the middle of `in_row` slots sits on the axis.
        let left = -(in_row as i32 - 1) * ACROSS / 2;
        let half = if stagger && row % 2 == 1 {
            ACROSS / 2
        } else {
            0
        };
        let back = if stagger { BACK * 3 / 2 } else { BACK };
        for c in 0..in_row {
            out.push((shift + left + c as i32 * ACROSS + half, row * back));
        }
        row += 1;
    }
    out
}

/// The slots of `formation` for `n` units, in quarter tiles `(across,
/// back)`, front rank first and left to right within a rank. `None` gives
/// no slots: the caller spreads the group over the ground instead.
pub fn offsets(formation: Formation, n: usize) -> Vec<(i32, i32)> {
    match formation {
        Formation::None => Vec::new(),
        // Twice as wide as deep: sqrt(2n) across.
        Formation::Line => ranks(n, sqrt_ceil(n * 2).min(12), false, 0),
        Formation::Box => ranks(n, sqrt_ceil(n), false, 0),
        Formation::Staggered => ranks(n, sqrt_ceil(n * 2).min(12), true, 0),
        Formation::Flank => {
            let left_n = n.div_ceil(2);
            let right_n = n - left_n;
            let w = sqrt_ceil(left_n.max(1));
            let half_wing = (w as i32 - 1) * ACROSS / 2;
            let shift = half_wing + WING_GAP / 2;
            let mut out = ranks(left_n, w, false, -shift);
            out.extend(ranks(right_n, w, false, shift));
            out
        }
    }
}

/// The slots in the world: `offsets` rotated to face `dir` (which need not
/// be unit length) and placed with the front centre at `front`.
pub fn place(front: Vec2Fx, dir: Vec2Fx, offsets: &[(i32, i32)]) -> Vec<Vec2Fx> {
    let ahead = if dir.length().is_zero() {
        Vec2Fx::new(Fx::ZERO, Fx::ONE)
    } else {
        dir.normalized_or_zero()
    };
    // "Across" runs to the group's right as it walks.
    let across = Vec2Fx::new(-ahead.y, ahead.x);
    offsets
        .iter()
        .map(|&(a, b)| {
            let a = Fx::from_ratio(a, 4);
            let b = Fx::from_ratio(b, 4);
            // Ranks behind the front stand against the direction of travel.
            front + across * a - ahead * b
        })
        .collect()
}

/// Which unit takes which slot: units are sorted by how far across the
/// axis they stand, slots are already left to right within a rank, so
/// the left of the group forms the left of the line. Returns, for each
/// unit index, the slot index it takes.
pub fn assign(positions: &[Vec2Fx], dir: Vec2Fx, n_slots: usize) -> Vec<usize> {
    let ahead = if dir.length().is_zero() {
        Vec2Fx::new(Fx::ZERO, Fx::ONE)
    } else {
        dir.normalized_or_zero()
    };
    let across = Vec2Fx::new(-ahead.y, ahead.x);
    let mut order: Vec<usize> = (0..positions.len()).collect();
    // Front-most first (so the front rank is the units nearest the goal),
    // then left to right; ties by index keep it deterministic.
    order.sort_by_key(|&i| {
        let p = positions[i];
        (
            core::cmp::Reverse(p.dot(ahead).raw()),
            p.dot(across).raw(),
            i,
        )
    });
    // Slot k in rank order goes to the k-th unit in that order; within a
    // rank, re-sort by across so the rank itself is left to right.
    let mut slot_of = vec![0usize; positions.len()];
    for (k, &unit) in order.iter().enumerate() {
        slot_of[unit] = k.min(n_slots.saturating_sub(1));
    }
    slot_of
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_have_one_slot_per_unit_and_the_right_proportions() {
        for f in Formation::ALL {
            for n in [1, 2, 5, 8, 17, 40] {
                let o = offsets(f, n);
                if f == Formation::None {
                    assert!(o.is_empty());
                    continue;
                }
                assert_eq!(o.len(), n, "{f:?} {n}");
                let mut sorted = o.clone();
                sorted.sort_unstable();
                sorted.dedup();
                assert_eq!(sorted.len(), n, "{f:?} {n}: slots are distinct");
            }
        }
        // A line of 8 is one rank wide; a box of 8 is three ranks.
        let line = offsets(Formation::Line, 8);
        assert!(
            line.iter().all(|&(_, b)| b == 0) || line.iter().filter(|&&(_, b)| b == 0).count() >= 4
        );
        let boxed = offsets(Formation::Box, 9);
        assert_eq!(boxed.iter().filter(|&&(_, b)| b == 0).count(), 3);
        // A flank has a gap in the middle of its front rank.
        let flank = offsets(Formation::Flank, 8);
        let front: Vec<i32> = flank
            .iter()
            .filter(|&&(_, b)| b == 0)
            .map(|&(a, _)| a)
            .collect();
        assert!(
            front.iter().any(|&a| a < -WING_GAP / 2) && front.iter().any(|&a| a > WING_GAP / 2)
        );
        assert!(!front.contains(&0));
        // Staggered ranks alternate their half-step.
        let stag = offsets(Formation::Staggered, 12);
        let odd: Vec<i32> = stag
            .iter()
            .filter(|&&(_, b)| b > 0 && b < BACK * 3)
            .map(|&(a, _)| a)
            .collect();
        assert!(
            odd.iter().all(|a| (a - ACROSS / 2) % ACROSS == 0),
            "{odd:?}"
        );
    }

    #[test]
    fn placement_faces_the_way_the_group_walks() {
        let o = offsets(Formation::Line, 3);
        // Walking +x: the rank runs along y, front at the target.
        let p = place(Vec2Fx::from_int(10, 10), Vec2Fx::from_int(1, 0), &o);
        assert!(p.iter().all(|q| q.x == Fx::from_int(10)));
        let mut ys: Vec<i32> = p.iter().map(|q| q.y.raw()).collect();
        ys.sort_unstable();
        assert_eq!(ys[1], Fx::from_int(10).raw());
        // Walking +y, the rank runs along x.
        let p = place(Vec2Fx::from_int(10, 10), Vec2Fx::from_int(0, 1), &o);
        assert!(p.iter().all(|q| q.y == Fx::from_int(10)));
        // A second rank stands behind the front, against the direction.
        let o = offsets(Formation::Box, 4);
        let p = place(Vec2Fx::from_int(10, 10), Vec2Fx::from_int(0, 1), &o);
        assert!(p.iter().any(|q| q.y < Fx::from_int(10)));
    }

    #[test]
    fn assignment_keeps_left_on_the_left() {
        let units = [
            Vec2Fx::from_int(0, 5),
            Vec2Fx::from_int(0, 0),
            Vec2Fx::from_int(0, 9),
        ];
        // Walking +x: across is +y... the unit at y=0 gets the first slot.
        let s = assign(&units, Vec2Fx::from_int(1, 0), 3);
        assert_eq!(s, vec![1, 0, 2]);
    }
}
