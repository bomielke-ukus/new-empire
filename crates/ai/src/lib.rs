//! Computer opponents (`docs/02` §12, `GD-AI-01`; `docs/07` D7).
//!
//! An [`Opponent`] is given a [`FoggedView`] each tick and answers with the
//! commands it wants issued: the same commands a player can give, from the
//! same view a player has. This crate depends on `fogged` and not on the
//! simulation, so it cannot read hidden state even by accident; the
//! compile-fail tests in `tests/` prove the world is unnameable from here
//! (`TA-AI-01`).
//!
//! The economy manager and the build orders are in [`economy`]; the
//! military manager follows in M5's later steps.

#![warn(missing_docs)]

pub mod economy;

use economy::{BuildOrder, Economy};
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
    /// Thoughts so far.
    thoughts: u64,
    /// What it aims for.
    order: BuildOrder,
    /// The economy manager.
    economy: Economy,
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
            order: BuildOrder::for_difficulty(difficulty),
            economy: Economy::default(),
        }
    }

    /// What it aims for.
    pub fn order(&self) -> &BuildOrder {
        &self.order
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

    /// One tick: the commands to issue this tick, in order. It thinks
    /// every `cadence` ticks of its order, on a tick of its own so two
    /// opponents do not think together, and answers with nothing between.
    pub fn think(&mut self, view: &FoggedView<'_>) -> Vec<Command> {
        debug_assert_eq!(view.player(), self.player, "a view of someone else's side");
        if !(view.tick() + u64::from(self.player)).is_multiple_of(self.order.cadence) {
            return Vec::new();
        }
        self.thoughts += 1;
        self.economy
            .think(view, &self.order, &mut self.rng)
            .into_iter()
            .map(|kind| view.command(kind))
            .collect()
    }

    /// How many times it has thought.
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
