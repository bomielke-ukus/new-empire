//! Where every unit of every resource came from and went: the books behind
//! the conservation invariant (`docs/09` §5).
//!
//! What a match holds of a resource is what lies on the map's nodes, what
//! villagers and boats carry, and what the sides have stockpiled. That only
//! changes by a flow the rules mean: a payment (less its refunds), a source
//! (a node spawned or a farm seeded, a trade boat's gold, a relic's gold, a
//! cheat, a scenario's gift) or a loss (a load dropped or lost with its
//! carrier, what was left on a node when it went, a scenario's taking). The
//! simulation records each flow where it happens, and
//! [`Simulation::check`](crate::Simulation::check) finds that
//!
//! ```text
//! held + spent + lost - made
//! ```
//!
//! has not moved since counting began. A duplicated load, a deposit counted
//! twice, a node that loses less than its gatherer takes: each moves it.
//!
//! The books are not state. They change nothing the simulation does, so they
//! are not hashed, not saved and invisible to equality. Counting begins at
//! the start of the next tick, so a fresh match, a clone and a loaded save
//! each start their own.

use crate::kinds::{Cost, Resource};

/// The books of one match. See the module documentation.
#[derive(Clone, Default, Debug)]
pub(crate) struct Ledger {
    /// The conserved total of each resource when counting began; `None`
    /// until the next tick begins it.
    base: Option<[i64; 4]>,
    /// Brought into the world.
    made: [i64; 4],
    /// Taken out of it other than by paying.
    lost: [i64; 4],
    /// Paid for things, less what was refunded.
    spent: [i64; 4],
}

impl PartialEq for Ledger {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for Ledger {}

impl Ledger {
    /// Whether counting has begun.
    pub(crate) fn counting(&self) -> bool {
        self.base.is_some()
    }

    /// Begins counting from what the match holds now.
    pub(crate) fn begin(&mut self, held: [i64; 4]) {
        self.base = Some(core::array::from_fn(|r| self.conserved(&held, r)));
    }

    fn conserved(&self, held: &[i64; 4], r: usize) -> i64 {
        held[r] + self.spent[r] + self.lost[r] - self.made[r]
    }

    /// `amount` of `r` brought into the world.
    pub(crate) fn made(&mut self, r: Resource, amount: i32) {
        self.made[r.index()] += amount as i64;
    }

    /// `amount` of `r` gone from the world other than by paying.
    pub(crate) fn lost(&mut self, r: Resource, amount: i32) {
        self.lost[r.index()] += amount as i64;
    }

    /// `cost` paid out of a stockpile.
    pub(crate) fn spent(&mut self, cost: &Cost) {
        for (s, c) in self.spent.iter_mut().zip(cost) {
            *s += *c as i64;
        }
    }

    /// `cost` handed back to a stockpile.
    pub(crate) fn refunded(&mut self, cost: &Cost) {
        for (s, c) in self.spent.iter_mut().zip(cost) {
            *s -= *c as i64;
        }
    }

    /// The first resource whose books do not balance against `held`, with
    /// the total counting began from and the total now; `None` if every
    /// one balances or counting has not begun.
    pub(crate) fn imbalance(&self, held: &[i64; 4]) -> Option<(Resource, i64, i64)> {
        let base = self.base?;
        Resource::ALL.into_iter().find_map(|r| {
            let now = self.conserved(held, r.index());
            (now != base[r.index()]).then_some((r, base[r.index()], now))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flows_balance_and_a_leak_does_not() {
        let mut l = Ledger::default();
        assert_eq!(l.imbalance(&[0; 4]), None, "not counting yet");
        l.begin([100, 50, 0, 0]);
        // A villager takes 10 wood off a tree: held is unchanged.
        assert_eq!(l.imbalance(&[100, 50, 0, 0]), None);
        // 30 wood paid for a house.
        l.spent(&[0, 30, 0, 0]);
        assert_eq!(l.imbalance(&[100, 20, 0, 0]), None);
        // Refunded on cancel.
        l.refunded(&[0, 30, 0, 0]);
        assert_eq!(l.imbalance(&[100, 50, 0, 0]), None);
        // A farm seeded with 250 food, a load of 7 lost with its villager.
        l.made(Resource::Food, 250);
        l.lost(Resource::Wood, 7);
        assert_eq!(l.imbalance(&[350, 43, 0, 0]), None);
        // Five food from nowhere.
        assert_eq!(
            l.imbalance(&[355, 43, 0, 0]),
            Some((Resource::Food, 100, 105))
        );
    }
}
