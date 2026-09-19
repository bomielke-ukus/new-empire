//! Notifications (`docs/03` §6.3): a stack of what just happened to the
//! player's side, newest at the bottom, each with a place to jump to
//! where there is one. Attacks are rate-limited by area (`UX-NOTIFY-01`)
//! so a long siege is one notice every twenty seconds, not an alarm loop.
//! Time is match ticks, so the stack stands still while the match does.

/// What a notice is about; the colour of its mark.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NoticeKind {
    /// Something of the side's is being hit.
    Attack,
    /// A unit or building of the side's is gone.
    Loss,
    /// A technology finished.
    Research,
    /// An age was reached.
    Age,
}

/// One line on the stack.
#[derive(Clone, PartialEq, Debug)]
pub struct Notice {
    /// What it is about.
    pub kind: NoticeKind,
    /// The line, in the font's capitals.
    pub text: String,
    /// Where it happened, in tiles, if anywhere: a click goes there.
    pub tile: Option<(f32, f32)>,
    /// The match tick it was raised at.
    pub tick: u64,
}

/// The most notices shown at once.
pub const SHOWN: usize = 5;
/// The most kept, shown or not.
const KEPT: usize = 40;
/// A notice leaves the stack after this many ticks: thirty seconds.
pub const LIFE_TICKS: u64 = 30 * sim::TICKS_PER_SECOND as u64;
/// Attacks within this many tiles of a recent attack notice are the same
/// attack.
pub const ATTACK_AREA: f32 = 12.0;
/// One attack notice per area per this many ticks: twenty seconds.
pub const ATTACK_TICKS: u64 = 20 * sim::TICKS_PER_SECOND as u64;

/// The stack.
#[derive(Clone, Default, Debug)]
pub struct Notices {
    items: Vec<Notice>,
}

impl Notices {
    /// Adds a notice. An attack within [`ATTACK_AREA`] tiles and
    /// [`ATTACK_TICKS`] of an attack already on the stack is not added.
    pub fn push(&mut self, notice: Notice) {
        if notice.kind == NoticeKind::Attack {
            let same_area = self.items.iter().any(|n| {
                n.kind == NoticeKind::Attack
                    && notice.tick.saturating_sub(n.tick) < ATTACK_TICKS
                    && near(n.tile, notice.tile)
            });
            if same_area {
                return;
            }
        }
        self.items.push(notice);
        if self.items.len() > KEPT {
            self.items.remove(0);
        }
    }

    /// Drops what has been up for [`LIFE_TICKS`] at `tick`.
    pub fn expire(&mut self, tick: u64) {
        self.items
            .retain(|n| tick.saturating_sub(n.tick) < LIFE_TICKS);
    }

    /// The notices on screen: the newest [`SHOWN`], oldest first.
    pub fn shown(&self) -> &[Notice] {
        let n = self.items.len();
        &self.items[n.saturating_sub(SHOWN)..]
    }

    /// Everything, for the tests.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing is up.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Nothing up: a new match.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

fn near(a: Option<(f32, f32)>, b: Option<(f32, f32)>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            let (dx, dy) = (a.0 - b.0, a.1 - b.1);
            dx * dx + dy * dy <= ATTACK_AREA * ATTACK_AREA
        }
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attack(x: f32, tick: u64) -> Notice {
        Notice {
            kind: NoticeKind::Attack,
            text: "UNDER ATTACK".into(),
            tile: Some((x, 10.0)),
            tick,
        }
    }

    /// Attacks in one area raise one notice in twenty seconds; another
    /// area, or twenty seconds later, raises its own. Other kinds stack
    /// freely, the stack shows its newest five, and a notice leaves after
    /// thirty seconds.
    ///
    /// REQ: UX-NOTIFY-01
    #[test]
    fn attacks_in_one_area_are_one_notice_per_twenty_seconds() {
        let mut n = Notices::default();
        n.push(attack(10.0, 100));
        n.push(attack(14.0, 200));
        n.push(attack(10.0, 100 + ATTACK_TICKS - 1));
        assert_eq!(n.len(), 1, "the same area within twenty seconds");
        n.push(attack(40.0, 200));
        assert_eq!(n.len(), 2, "another area");
        n.push(attack(10.0, 100 + ATTACK_TICKS));
        assert_eq!(n.len(), 3, "twenty seconds on");
        for i in 0..6 {
            n.push(Notice {
                kind: NoticeKind::Loss,
                text: format!("HOUSE {i} DESTROYED"),
                tile: Some((1.0, 1.0)),
                tick: 300,
            });
        }
        assert_eq!(n.len(), 9);
        let shown = n.shown();
        assert_eq!(shown.len(), SHOWN);
        assert_eq!(shown[SHOWN - 1].text, "HOUSE 5 DESTROYED", "newest last");
        n.expire(300 + LIFE_TICKS - 1);
        assert_eq!(
            n.len(),
            7,
            "the two earliest attacks are gone, the third stays"
        );
        n.expire(100 + ATTACK_TICKS + LIFE_TICKS);
        assert!(n.is_empty());
        n.push(Notice {
            kind: NoticeKind::Research,
            text: "STONE MINING RESEARCHED".into(),
            tile: None,
            tick: 1,
        });
        n.push(Notice {
            kind: NoticeKind::Research,
            text: "STONE MINING RESEARCHED".into(),
            tile: None,
            tick: 1,
        });
        assert_eq!(n.len(), 2, "only attacks are rate-limited");
        n.clear();
        assert!(n.shown().is_empty());
    }
}
