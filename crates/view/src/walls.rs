//! How a wall's tiles join up on screen.
//!
//! A wall is laid a tile at a time, and each tile is its own entity. A
//! rendered wall set draws a tile as its post plus an arm toward each wall
//! of the same owner beside it, and the arms of two tiles meet at the edge
//! or the corner they share (`tools/render/kit.py`, `WALL_DIRECTIONS`). A
//! gate stands across the line of the walls it is set into, in one of four
//! orientations. This module decides which arms a tile shows and which way
//! a gate faces; the scene places the frames.

/// The eight neighbours a wall tile can join, as tile offsets, in the order
/// its set's arms are rendered. A gate's orientation is an index into the
/// first four: the line its walls run along.
pub const WALL_DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// What stands on a tile, as far as joining goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Piece {
    /// A palisade or stone wall.
    Wall,
    /// A gate.
    Gate,
}

/// The line (0 to 3) a gate at `t` stands across: the first of the game's
/// x, y and the two diagonals with a piece on both sides, else on one
/// side, else x. `at` says what of the gate's owner stands on a tile.
pub fn gate_line(t: (i32, i32), at: &impl Fn(i32, i32) -> Option<Piece>) -> u8 {
    let has = |k: usize| {
        let (dx, dy) = WALL_DIRECTIONS[k];
        at(t.0 + dx, t.1 + dy).is_some()
    };
    const ORDER: [usize; 4] = [0, 2, 1, 3];
    ORDER
        .into_iter()
        .find(|&o| has(o) && has(o + 4))
        .or_else(|| ORDER.into_iter().find(|&o| has(o) || has(o + 4)))
        .unwrap_or(0) as u8
}

/// Which of its eight neighbours the wall at `t` joins, as a bit per
/// [`WALL_DIRECTIONS`] index. It joins every wall of its owner beside it,
/// and a gate whose line points at it. A diagonal neighbour is joined only
/// when neither tile between them holds a piece, so a corner turns through
/// the tile on it rather than cutting across. Both rules are symmetric:
/// two walls either both reach for each other or neither does.
pub fn wall_links(t: (i32, i32), at: &impl Fn(i32, i32) -> Option<Piece>) -> u8 {
    let mut mask = 0;
    for (k, &(dx, dy)) in WALL_DIRECTIONS.iter().enumerate() {
        let (x, y) = (t.0 + dx, t.1 + dy);
        let Some(piece) = at(x, y) else {
            continue;
        };
        if dx != 0 && dy != 0 && (at(t.0 + dx, t.1).is_some() || at(t.0, t.1 + dy).is_some()) {
            continue;
        }
        // From the gate, this wall lies in direction k + 4: on its line
        // when that is the gate's orientation, either way along it.
        if piece == Piece::Gate && gate_line((x, y), at) != (k % 4) as u8 {
            continue;
        }
        mask |= 1 << k;
    }
    mask
}

/// Where arm `k` sits in the draw order, relative to its post. An arm
/// reaching toward the viewer is in front of the post; one reaching away is
/// behind it. One reaching sideways on screen is drawn first too: a
/// diagonal arm's inner end is cut to the pier's corner, and that cut faces
/// the viewer from behind the pier, which has to cover it.
pub fn arm_depth(k: usize) -> f32 {
    let (dx, dy) = WALL_DIRECTIONS[k];
    match dx + dy {
        0 => -0.01,
        s => s as f32 * 0.25,
    }
}

/// The direction index from tile `a` to its neighbour `b`, if they touch.
pub fn direction(a: (i32, i32), b: (i32, i32)) -> Option<usize> {
    let d = (b.0 - a.0, b.1 - a.1);
    WALL_DIRECTIONS.iter().position(|&w| w == d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn layout(pieces: &[((i32, i32), Piece)]) -> impl Fn(i32, i32) -> Option<Piece> {
        let map: HashMap<(i32, i32), Piece> = pieces.iter().copied().collect();
        move |x, y| map.get(&(x, y)).copied()
    }

    fn walls(tiles: &[(i32, i32)]) -> impl Fn(i32, i32) -> Option<Piece> {
        layout(&tiles.iter().map(|&t| (t, Piece::Wall)).collect::<Vec<_>>())
    }

    fn bits(k: &[usize]) -> u8 {
        k.iter().fold(0, |m, &k| m | 1 << k)
    }

    #[test]
    fn a_straight_run_joins_end_to_end_and_its_ends_reach_one_way() {
        let at = walls(&[(0, 0), (1, 0), (2, 0)]);
        assert_eq!(wall_links((0, 0), &at), bits(&[0]));
        assert_eq!(wall_links((1, 0), &at), bits(&[0, 4]));
        assert_eq!(wall_links((2, 0), &at), bits(&[4]));
        assert_eq!(wall_links((5, 5), &at), 0, "a lone tile is its post");
    }

    #[test]
    fn a_diagonal_run_joins_corner_to_corner() {
        let at = walls(&[(0, 0), (1, 1), (2, 2)]);
        assert_eq!(wall_links((1, 1), &at), bits(&[1, 5]));
        let at = walls(&[(0, 0), (1, -1)]);
        assert_eq!(wall_links((0, 0), &at), bits(&[7]));
        assert_eq!(wall_links((1, -1), &at), bits(&[3]));
    }

    #[test]
    fn a_corner_turns_through_its_tile_rather_than_cutting_across() {
        // (0,0) - (1,0)
        //           |
        //         (1,1)
        let at = walls(&[(0, 0), (1, 0), (1, 1)]);
        assert_eq!(wall_links((0, 0), &at), bits(&[0]), "no shortcut to (1,1)");
        assert_eq!(wall_links((1, 0), &at), bits(&[2, 4]));
        assert_eq!(wall_links((1, 1), &at), bits(&[6]));
    }

    #[test]
    fn joining_is_symmetric() {
        let tiles = [
            (0, 0),
            (1, 0),
            (1, 1),
            (2, 2),
            (3, 1),
            (0, 1),
            (-1, 2),
            (4, 1),
        ];
        let at = walls(&tiles);
        for &a in &tiles {
            for &b in &tiles {
                if let Some(k) = direction(a, b) {
                    let ab = wall_links(a, &at) & (1 << k) != 0;
                    let ba = wall_links(b, &at) & (1 << ((k + 4) % 8)) != 0;
                    assert_eq!(ab, ba, "{a:?} and {b:?}");
                }
            }
        }
    }

    #[test]
    fn a_gate_faces_along_its_walls_and_joins_only_along_that_line() {
        let at = layout(&[
            ((0, 0), Piece::Wall),
            ((1, 0), Piece::Gate),
            ((2, 0), Piece::Wall),
            ((1, 1), Piece::Wall),
        ]);
        assert_eq!(gate_line((1, 0), &at), 0, "walls either side along x");
        assert_eq!(wall_links((0, 0), &at), bits(&[0]));
        assert_eq!(wall_links((2, 0), &at), bits(&[4]));
        assert_eq!(
            wall_links((1, 1), &at),
            0,
            "the wall behind the gate's side does not reach into it"
        );
        let at = layout(&[
            ((0, 0), Piece::Wall),
            ((0, 1), Piece::Gate),
            ((0, 2), Piece::Wall),
        ]);
        assert_eq!(gate_line((0, 1), &at), 2);
        let at = layout(&[((0, 0), Piece::Wall), ((1, -1), Piece::Gate)]);
        assert_eq!(gate_line((1, -1), &at), 3, "one wall decides it");
        assert_eq!(wall_links((0, 0), &at), bits(&[7]));
        assert_eq!(gate_line((9, 9), &at), 0, "alone, it stands across x");
    }

    #[test]
    fn arms_toward_the_viewer_draw_over_the_post_and_the_rest_under_it() {
        assert!(arm_depth(0) > 0.0 && arm_depth(1) > 0.0 && arm_depth(2) > 0.0);
        assert!(arm_depth(4) < 0.0 && arm_depth(5) < 0.0 && arm_depth(6) < 0.0);
        assert!(arm_depth(3) < 0.0 && arm_depth(7) < 0.0);
        // Between the post and the neighbour's arm reaching back for it.
        for (k, &(dx, dy)) in WALL_DIRECTIONS.iter().enumerate() {
            let toward = (dx + dy) as f32;
            if toward != 0.0 {
                assert!(
                    arm_depth(k) / toward > 0.0 && arm_depth(k) / toward < 0.5,
                    "{k}"
                );
            }
        }
    }
}
