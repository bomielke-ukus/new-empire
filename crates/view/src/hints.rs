//! First-time hints (`docs/03` §7): one line at a time, in context, each
//! shown at most twice to a player, all of them switchable off. The rules
//! are here and pure; the app reads the match for the conditions and keeps
//! the counts in the settings file.

use std::collections::BTreeMap;

/// A hint the game can offer.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Hint {
    /// A villager is selected and nobody gathers yet.
    Gather,
    /// Villagers have stood idle a while.
    IdleVillagers,
    /// The population is at its cap.
    Housed,
    /// The next age can be researched now.
    AgeWithinReach,
    /// The side has just been attacked for the first time.
    UnderAttack,
}

impl Hint {
    /// Every hint, in the order they win when several are due: the urgent
    /// first.
    pub const ALL: [Hint; 5] = [
        Hint::UnderAttack,
        Hint::Housed,
        Hint::IdleVillagers,
        Hint::AgeWithinReach,
        Hint::Gather,
    ];

    /// The name the settings file counts it under.
    pub const fn id(self) -> &'static str {
        match self {
            Hint::Gather => "gather",
            Hint::IdleVillagers => "idle-villagers",
            Hint::Housed => "housed",
            Hint::AgeWithinReach => "age-within-reach",
            Hint::UnderAttack => "under-attack",
        }
    }

    /// The hint with that name.
    pub fn from_id(id: &str) -> Option<Hint> {
        Hint::ALL.into_iter().find(|h| h.id() == id)
    }

    /// The line, with the keys as the player has them bound.
    pub fn text(self, keys: &Keys) -> String {
        match self {
            Hint::Gather => "RIGHT-CLICK A TREE, BUSH OR VEIN TO GATHER FROM IT".to_string(),
            Hint::IdleVillagers => {
                format!("VILLAGERS ARE IDLE: PRESS {} TO FIND THEM", keys.next_idle)
            }
            Hint::Housed => "YOU ARE HOUSED: SELECT A VILLAGER AND PRESS H FOR A HOUSE".to_string(),
            Hint::AgeWithinReach => {
                "THE NEXT AGE IS WITHIN REACH: SELECT THE TOWN CENTER".to_string()
            }
            Hint::UnderAttack => format!(
                "UNDER ATTACK: CLICK THE NOTICE TO LOOK, {} TO PAUSE",
                keys.pause
            ),
        }
    }
}

/// The keys a hint names, as the settings show them.
#[derive(Clone, Debug)]
pub struct Keys {
    /// The next-idle-villager key.
    pub next_idle: String,
    /// The pause key.
    pub pause: String,
}

impl Default for Keys {
    fn default() -> Keys {
        Keys {
            next_idle: ".".to_string(),
            pause: "SPACE".to_string(),
        }
    }
}

/// What the match looks like this moment, as far as the hints care.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Conditions {
    /// One of the player's villagers is selected.
    pub villager_selected: bool,
    /// Something of the player's is gathering.
    pub anyone_gathering: bool,
    /// How many of the player's villagers stand idle.
    pub idle_villagers: usize,
    /// The population is at its cap.
    pub housed: bool,
    /// The next age can be researched now.
    pub age_within_reach: bool,
    /// The side has been attacked.
    pub attacked: bool,
}

/// The conditions as a match presents them, for `me`: whether one of their
/// villagers is selected and whether they have just been attacked are the
/// caller's to know.
pub fn conditions(
    sim: &sim::Simulation,
    me: u8,
    villager_selected: bool,
    attacked: bool,
) -> Conditions {
    use sim::{kinds, tech, Order};
    let world = sim.world();
    let anyone_gathering = world.slots().any(|s| {
        world.owner[s.index()] == me && matches!(world.order[s.index()], Order::Gather { .. })
    });
    let (housed, age_within_reach) = match sim.player(me) {
        Some(pl) => {
            let tc = world.slots().find(|s| {
                world.owner[s.index()] == me
                    && world.kind[s.index()] == kinds::TOWN_CENTER
                    && world.construction[s.index()].is_none()
            });
            let reach = match (tc, tech::age_advance(pl.age)) {
                (Some(tc), Some(t)) => sim.can_research(me, world.id_at(tc), t.id).is_ok(),
                _ => false,
            };
            (pl.pop >= pl.pop_cap, reach)
        }
        None => (false, false),
    };
    Conditions {
        villager_selected,
        anyone_gathering,
        idle_villagers: sim.idle_villagers(me).len(),
        housed,
        age_within_reach,
        attacked,
    }
}

/// A hint stays up this long: ten seconds.
pub const SHOW_TICKS: u64 = 10 * sim::TICKS_PER_SECOND as u64;
/// And the next waits this long after it: twenty seconds.
pub const GAP_TICKS: u64 = 20 * sim::TICKS_PER_SECOND as u64;
/// Villagers must have been idle this long before the hint: five seconds.
pub const IDLE_TICKS: u64 = 5 * sim::TICKS_PER_SECOND as u64;
/// A hint is shown at most this often, ever.
pub const MAX_SHOWN: u8 = 2;

/// The hints' state for one player.
#[derive(Clone, Debug)]
pub struct Hints {
    /// Off means none, ever.
    pub enabled: bool,
    shown: BTreeMap<Hint, u8>,
    current: Option<(Hint, u64)>,
    last_end: Option<u64>,
    idle_since: Option<u64>,
}

impl Hints {
    /// From the settings: whether they are on, and how often each has
    /// been shown already.
    pub fn new(enabled: bool, counts: &BTreeMap<String, u8>) -> Hints {
        let shown = counts
            .iter()
            .filter_map(|(id, n)| Hint::from_id(id).map(|h| (h, *n)))
            .collect();
        Hints {
            enabled,
            shown,
            current: None,
            last_end: None,
            idle_since: None,
        }
    }

    /// A new match: nothing up, nothing timed; the counts are the player's
    /// and stay.
    pub fn reset(&mut self) {
        self.current = None;
        self.last_end = None;
        self.idle_since = None;
    }

    /// The counts, for the settings file.
    pub fn counts(&self) -> BTreeMap<String, u8> {
        self.shown
            .iter()
            .map(|(h, n)| (h.id().to_string(), *n))
            .collect()
    }

    /// The hint up at `tick`, if one is.
    pub fn showing(&self, tick: u64) -> Option<Hint> {
        self.current
            .filter(|(_, since)| tick.saturating_sub(*since) < SHOW_TICKS)
            .map(|(h, _)| h)
    }

    /// Reads the moment. The hint that starts now, if one does: the caller
    /// records it. One at a time, a gap between, each at most twice.
    pub fn update(&mut self, c: &Conditions, tick: u64) -> Option<Hint> {
        if c.idle_villagers > 0 {
            self.idle_since.get_or_insert(tick);
        } else {
            self.idle_since = None;
        }
        if !self.enabled {
            self.current = None;
            return None;
        }
        if let Some((_, since)) = self.current {
            if tick.saturating_sub(since) < SHOW_TICKS {
                return None;
            }
            self.current = None;
            self.last_end = Some(since + SHOW_TICKS);
        }
        if self
            .last_end
            .is_some_and(|end| tick.saturating_sub(end) < GAP_TICKS)
        {
            return None;
        }
        let due = |h: Hint| match h {
            Hint::Gather => c.villager_selected && !c.anyone_gathering,
            Hint::IdleVillagers => self
                .idle_since
                .is_some_and(|s| tick.saturating_sub(s) >= IDLE_TICKS),
            Hint::Housed => c.housed,
            Hint::AgeWithinReach => c.age_within_reach,
            Hint::UnderAttack => c.attacked,
        };
        let next = Hint::ALL
            .into_iter()
            .find(|h| due(*h) && self.shown.get(h).copied().unwrap_or(0) < MAX_SHOWN)?;
        *self.shown.entry(next).or_insert(0) += 1;
        self.current = Some((next, tick));
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hint comes when its moment does, stays ten seconds, the next
    /// waits twenty more, the urgent wins when several are due, each is
    /// shown at most twice ever and the counts survive a new match, and
    /// off is off.
    #[test]
    fn hints_come_in_context_one_at_a_time_at_most_twice_and_can_be_off() {
        let mut h = Hints::new(true, &BTreeMap::new());
        let quiet = Conditions::default();
        assert_eq!(h.update(&quiet, 0), None, "nothing is due");
        let selected = Conditions {
            villager_selected: true,
            ..quiet
        };
        assert_eq!(h.update(&selected, 10), Some(Hint::Gather));
        assert_eq!(h.showing(10), Some(Hint::Gather));
        assert_eq!(h.showing(10 + SHOW_TICKS - 1), Some(Hint::Gather));
        assert_eq!(h.showing(10 + SHOW_TICKS), None, "ten seconds");
        // Idle villagers and an attack both due: the attack first, but
        // only after the gap.
        let busy = Conditions {
            idle_villagers: 2,
            attacked: true,
            ..quiet
        };
        assert_eq!(h.update(&busy, 10 + SHOW_TICKS), None, "the gap");
        let after_gap = 10 + SHOW_TICKS + GAP_TICKS;
        assert_eq!(h.update(&busy, after_gap), Some(Hint::UnderAttack));
        // The idle hint waits for five seconds of idleness, counted from
        // when the idleness began, then comes after the attack's turn once
        // the attack is no longer news.
        let later = after_gap + SHOW_TICKS + GAP_TICKS;
        let calm = Conditions {
            attacked: false,
            ..busy
        };
        assert_eq!(h.update(&calm, later), Some(Hint::IdleVillagers));
        // Twice, ever.
        let mut t = later + SHOW_TICKS + GAP_TICKS;
        assert_eq!(
            h.update(&selected, t),
            Some(Hint::Gather),
            "the second time"
        );
        t += SHOW_TICKS + GAP_TICKS;
        assert_eq!(h.update(&selected, t), None, "never a third");
        let counts = h.counts();
        assert_eq!(counts.get("gather"), Some(&2));
        assert_eq!(counts.get("under-attack"), Some(&1));
        let mut again = Hints::new(true, &counts);
        assert_eq!(
            again.update(&selected, 0),
            None,
            "the counts are the player's"
        );
        let housed = Conditions {
            housed: true,
            ..quiet
        };
        assert_eq!(again.update(&housed, 0), Some(Hint::Housed));
        again.reset();
        assert_eq!(again.showing(1), None, "a new match starts clear");
        assert_eq!(
            again.counts().get("housed"),
            Some(&1),
            "but keeps the counts"
        );
        let mut off = Hints::new(false, &BTreeMap::new());
        assert_eq!(off.update(&housed, 0), None);
        assert!(off.counts().is_empty());
        let keys = Keys::default();
        assert!(Hint::IdleVillagers.text(&keys).contains("PRESS ."));
        assert!(Hint::UnderAttack.text(&keys).contains("SPACE"));
        for hint in Hint::ALL {
            assert_eq!(Hint::from_id(hint.id()), Some(hint));
        }
    }
}
