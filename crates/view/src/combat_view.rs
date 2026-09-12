//! Words for a unit's fighting numbers, as the panel and tooltips say them.

use sim::kinds::KindInfo;
use sim::{combat, Modifiers};

/// "ATTACK 5 MELEE" or "ATTACK 6 PIERCE, RANGE 6", after technology.
pub fn attack_words(k: &KindInfo, m: &Modifiers) -> String {
    let mut t = format!(
        "ATTACK {} {}",
        combat::attack_of(k, m),
        k.combat.damage.name().to_uppercase()
    );
    let range = combat::range_of(k, m);
    if range > 0 {
        t.push_str(&format!(", RANGE {range}"));
    }
    t
}

/// "ARMOUR 1/1" as melee/pierce, after technology.
pub fn armour_words(k: &KindInfo, m: &Modifiers) -> String {
    let a = combat::armour_of(k, m);
    format!("ARMOUR {}/{}", a.melee, a.pierce)
}
