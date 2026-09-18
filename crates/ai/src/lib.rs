//! Computer opponents (`docs/02` §12, `GD-AI-01`; `docs/07` D7).
//!
//! An [`Opponent`] is given a [`FoggedView`] each tick and answers with the
//! commands it wants issued: the same commands a player can give, from the
//! same view a player has. This crate depends on `fogged` and not on the
//! simulation, so it cannot read hidden state even by accident; the
//! compile-fail tests in `tests/` prove the world is unnameable from here
//! (`TA-AI-01`).
//!
//! This is the boundary and an opponent that does nothing. Build orders,
//! the economy and the military managers follow in M5's later steps.

#![warn(missing_docs)]

use fogged::{Command, FoggedView, PlayerId, Rng};

/// How hard the opponent tries (`docs/02` §12). Only Hardest is allowed
/// anything a player is not, and it says so in the UI.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Difficulty {
    /// Simple build order, small attacks, slow reactions, no raids.
    Easy,
    /// Solid build order, scouts, expands, counters, raids.
    #[default]
    Standard,
    /// Faster decisions, multi-pronged attacks, targets the economy.
    Hard,
    /// Hard, plus declared resource bonuses.
    Hardest,
}

impl Difficulty {
    /// Every level, easiest first.
    pub const ALL: [Difficulty; 4] = [
        Difficulty::Easy,
        Difficulty::Standard,
        Difficulty::Hard,
        Difficulty::Hardest,
    ];

    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Standard => "Standard",
            Difficulty::Hard => "Hard",
            Difficulty::Hardest => "Hardest",
        }
    }
}

/// One computer player.
#[derive(Clone, Debug)]
pub struct Opponent {
    player: PlayerId,
    difficulty: Difficulty,
    /// Its own dice, seeded from the match so a match replays identically.
    rng: Rng,
    /// Ticks thought so far.
    thoughts: u64,
}

impl Opponent {
    /// An opponent for `player`, with dice seeded from the match `seed`
    /// and its own number, so two opponents in one match roll differently
    /// and the same match rolls the same twice.
    pub fn new(player: PlayerId, difficulty: Difficulty, seed: u64) -> Opponent {
        Opponent {
            player,
            difficulty,
            rng: Rng::new(seed ^ (0xA1 + u64::from(player)).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
            thoughts: 0,
        }
    }

    /// Whose side it plays.
    pub fn player(&self) -> PlayerId {
        self.player
    }

    /// How hard it tries.
    pub fn difficulty(&self) -> Difficulty {
        self.difficulty
    }

    /// The dice, for the managers that follow.
    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }

    /// One tick of thought: the commands to issue this tick, in order.
    /// Nothing yet: the boundary is in place before the behaviour behind it.
    pub fn think(&mut self, view: &FoggedView<'_>) -> Vec<Command> {
        debug_assert_eq!(view.player(), self.player, "a view of someone else's side");
        self.thoughts += 1;
        Vec::new()
    }

    /// How many ticks it has thought.
    pub fn thoughts(&self) -> u64 {
        self.thoughts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opponents_are_seeded_apart_and_repeat_themselves() {
        let mut a = Opponent::new(1, Difficulty::Standard, 7);
        let mut b = Opponent::new(2, Difficulty::Standard, 7);
        let mut again = Opponent::new(1, Difficulty::Standard, 7);
        let roll = |o: &mut Opponent| (0..8).map(|_| o.rng().below(1000)).collect::<Vec<_>>();
        let (ra, rb, rc) = (roll(&mut a), roll(&mut b), roll(&mut again));
        assert_ne!(ra, rb, "two sides, two sets of dice");
        assert_eq!(ra, rc, "the same side rolls the same");
        assert_eq!(a.player(), 1);
        assert_eq!(a.difficulty().name(), "Standard");
        assert_eq!(Difficulty::ALL.len(), 4);
        assert_eq!(Difficulty::default(), Difficulty::Standard);
    }
}
