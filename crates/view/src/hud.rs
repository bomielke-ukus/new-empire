//! The heads-up display: resource bar, selection panel, command grid, queue
//! strip, health bars and the age banner. Produces screen-space sprites and
//! the buttons' hit rectangles; the app decides what a click means.

use sim::entity::{KindId, Slot};
use sim::kinds::{self, Cost, KindInfo, Resource};
use sim::tech::{self, TechId, TechInfo};
use sim::{EntityId, Formation, GatherPhase, Item, Order, Simulation, Stance};

use crate::camera::Camera;
use crate::font;
use crate::fx_to_f32;
use crate::iso;
use crate::palette::*;
use crate::scene::SpriteInstance;
use crate::sprites::{Atlas, Frame, Ink};

/// Height of the top resource bar.
pub const TOP_BAR: f32 = 26.0;
/// Height of the bottom panel.
pub const BOTTOM_PANEL: f32 = 132.0;
/// Width reserved on the right of the bottom panel for the minimap.
pub const MINIMAP_RESERVE: f32 = 300.0;
/// The command grid (`docs/03` §3): five across, three down.
pub const GRID_COLS: usize = 5;
/// Rows in the command grid.
pub const GRID_ROWS: usize = 3;
/// Widest a command button gets.
const BUTTON_W: f32 = 100.0;
/// Narrowest a command button gets before buttons are dropped instead.
const BUTTON_MIN_W: f32 = 48.0;
/// Command button height.
const BUTTON_H: f32 = 30.0;
/// Gap between command buttons.
const BUTTON_GAP: f32 = 4.0;

/// What a button does when clicked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Enter placement mode for a building.
    Build(KindId),
    /// Queue a unit at the selected building that trains it.
    Train(KindId),
    /// Stop the selected units.
    Stop,
    /// Leave placement mode.
    Cancel,
    /// Remove the last queued item at the building whose queue is displayed.
    CancelTrain(EntityId),
    /// Queue a technology (an age advance included) at the selected building.
    Research(TechId),
    /// Flip the player's farm auto-reseed.
    ToggleReseed,
    /// Start picking a point to attack-move to (`UX-CMD-02`).
    AttackMove,
    /// Start picking a point to patrol to (`UX-CMD-03`).
    Patrol,
    /// Set the selected units' stance (`UX-CMD-07`).
    Stance(Stance),
    /// Set the selected units' formation (`UX-CMD-08`).
    Formation(Formation),
}

/// A clickable region.
#[derive(Clone, PartialEq, Debug)]
pub struct Button {
    /// Window px.
    pub x: f32,
    /// Window px.
    pub y: f32,
    /// Size.
    pub w: f32,
    /// Size.
    pub h: f32,
    /// What it does.
    pub action: Action,
    /// Label text.
    pub label: String,
    /// Cost shorthand, shown under the label; empty when free.
    pub cost: String,
    /// Hotkey shown on the button.
    pub hotkey: char,
    /// Whether clicking does anything right now.
    pub enabled: bool,
    /// Why not, when it does not; the cost and full name when it does.
    pub reason: String,
}

impl Button {
    /// True if a window point is inside.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Everything the HUD needs to draw a frame.
pub struct HudInput<'a> {
    /// The match.
    pub sim: &'a Simulation,
    /// Whose HUD.
    pub player: u8,
    /// For projecting health bars.
    pub camera: &'a Camera,
    /// Selected entity slots.
    pub selected: &'a [u32],
    /// Building being placed, if any.
    pub build_mode: Option<KindId>,
    /// Frames per second, for the corner readout.
    pub fps: f32,
    /// Whether the clock is paused.
    pub paused: bool,
    /// Game speed multiplier.
    pub speed: f32,
    /// Window title-style status; shown top right.
    pub status: &'a str,
    /// Cursor position in window (device) pixels, for button hover and the
    /// tooltip line.
    pub hover: Option<(f32, f32)>,
    /// A line to celebrate across the top of the world, if any.
    pub banner: Option<&'a str>,
    /// Device pixels per HUD pixel: the display scale times the player's
    /// UI scale. The HUD is laid out in its own pixels and scaled up on
    /// the way out, so text is the same size on any display.
    pub ui_scale: f32,
    /// Whether the controls overlay is open.
    pub help: bool,
    /// Whether the player is picking a point for an attack-move or patrol.
    pub targeting: bool,
}

/// How long the "F1 CONTROLS" hint stays in the resource bar: the first
/// minute of a match.
pub const HINT_TICKS: u64 = 60 * sim::TICKS_PER_SECOND as u64;

/// True while the resource bar should point at the controls overlay.
pub fn controls_hint(sim: &Simulation) -> bool {
    sim.tick() < HINT_TICKS
}

/// Every control, as two columns of `(key, what it does)`: the general
/// controls, and the build, train and research keys. The second column
/// comes from the same tables the command grid uses, so it cannot drift.
pub fn controls() -> [Vec<(String, String)>; 2] {
    let s = |k: &str, a: &str| (k.to_string(), a.to_string());
    let general = vec![
        s("WASD", "PAN THE CAMERA"),
        s("MID DRAG", "PAN"),
        s("WHEEL +-", "ZOOM 0.5X TO 3X"),
        s("MINIMAP", "CLICK JUMPS, RIGHT-CLICK SENDS"),
        s("CLICK", "SELECT, DRAG FOR A BOX"),
        s("DBL CLICK", "ALL OF A KIND ON SCREEN"),
        s("SHIFT", "ADD TO THE SELECTION"),
        s("CTRL+0-9", "SAVE A GROUP, 0-9 RECALLS"),
        s(".", "NEXT IDLE VILLAGER"),
        s("RIGHT", "MOVE, GATHER, BUILD, RALLY"),
        s("T", "STOP"),
        s("C P G B L", "TRAIN AT A BARRACKS, RANGE, STABLE"),
        s("RIGHT", "ON AN ENEMY: ATTACK"),
        s("M, P", "ATTACK-MOVE, PATROL, THEN CLICK"),
        s("Q E I K", "STANCE, AGGRESSIVE TO PASSIVE"),
        s("Z", "NEXT FORMATION"),
        s("DELETE", "DISMISS"),
        s("SPACE", "PAUSE"),
        s("[ ]", "SLOWER, FASTER"),
        s("SHIFT+E", "EDGE SCROLL ON, OFF"),
        s("F2", "HUD SIZE"),
        s("ESC", "CANCEL, DESELECT, QUIT"),
    ];
    let mut build: Vec<&KindInfo> = kinds::all()
        .iter()
        .filter(|k| k.buildable && !k.mobile && k.id != kinds::TOWN_CENTER)
        .collect();
    build.sort_by_key(|k| (k.age, k.id));
    let mut orders: Vec<(String, String)> = build
        .iter()
        .map(|k| {
            let age = if k.age == sim::Age::Stone {
                String::new()
            } else {
                format!(
                    " ({})",
                    k.age.name().trim_end_matches(" Age").to_uppercase()
                )
            };
            (
                build_hotkey(k.id).to_string(),
                format!("{} {}{age}", short_name(k.id), cost_label(&k.cost)),
            )
        })
        .collect();
    orders.push(s("V", "TRAIN A VILLAGER"));
    orders.push(s("U", "ADVANCE THE AGE"));
    let keys: String = TECH_KEYS
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    orders.push((keys, "RESEARCH, IN GRID ORDER".to_string()));
    orders.push(s("R", "AUTO-RESEED ON, OFF"));
    orders.push(s("X", "UNQUEUE, OR CANCEL PLACING"));
    orders.push(s("SHIFT", "KEEP PLACING"));
    [general, orders]
}

/// A built HUD.
#[derive(Clone, Default, Debug)]
pub struct Hud {
    /// Screen-space sprites in draw order.
    pub sprites: Vec<SpriteInstance>,
    /// Clickable buttons.
    pub buttons: Vec<Button>,
}

/// Draws screen-space primitives into a sprite list.
pub struct Painter<'a> {
    atlas: &'a Atlas,
    /// Output.
    pub out: Vec<SpriteInstance>,
}

impl<'a> Painter<'a> {
    /// A painter over an atlas.
    pub fn new(atlas: &'a Atlas) -> Painter<'a> {
        Painter {
            atlas,
            out: Vec::new(),
        }
    }

    fn push(&mut self, f: &Frame, x: f32, y: f32, w: f32, h: f32, row: u8) {
        self.out.push(SpriteInstance {
            x: x.round(),
            y: y.round(),
            w,
            h,
            u: f.x,
            v: f.y,
            uw: f.w,
            vh: f.h,
            row,
            flip: false,
            depth: 0.0,
            slot: u32::MAX,
            screen: true,
        });
    }

    /// A filled rectangle in a palette colour (player row 0 unless the
    /// colour is a player ramp index, in which case `row` applies).
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, idx: u8, row: u8) {
        let f = *self.atlas.solid(idx);
        self.push(&f, x, y, w, h, row);
    }

    /// A rectangle outline one pixel wide.
    pub fn outline(&mut self, x: f32, y: f32, w: f32, h: f32, idx: u8) {
        self.rect(x, y, w, 1.0, idx, 0);
        self.rect(x, y + h - 1.0, w, 1.0, idx, 0);
        self.rect(x, y, 1.0, h, idx, 0);
        self.rect(x + w - 1.0, y, 1.0, h, idx, 0);
    }

    /// Text at a pixel position, at an integer scale. Returns the width drawn.
    pub fn text(&mut self, x: f32, y: f32, text: &str, dark: bool, scale: f32) -> f32 {
        self.text_in(
            x,
            y,
            text,
            if dark { Ink::Black } else { Ink::White },
            scale,
        )
    }

    /// Text in an ink. Returns the width drawn.
    pub fn text_in(&mut self, x: f32, y: f32, text: &str, ink: Ink, scale: f32) -> f32 {
        let mut cx = x;
        for ch in text.chars() {
            if let Some(g) = self.atlas.glyph_ink(ch, ink) {
                let g = *g;
                if ch != ' ' {
                    self.push(&g, cx, y, g.w as f32 * scale, g.h as f32 * scale, 0);
                }
            }
            cx += font::ADVANCE as f32 * scale;
        }
        cx - x
    }

    /// A labelled button. Greyed when disabled; lit when hovered.
    pub fn button(&mut self, b: &Button, hover: bool) {
        let (fill, ink) = if !b.enabled {
            (GREY_DARK, Ink::White)
        } else if hover {
            (TAN, Ink::Black)
        } else {
            (BROWN, Ink::Black)
        };
        self.rect(b.x, b.y, b.w, b.h, BROWN_DARK, 0);
        self.rect(b.x + 1.0, b.y + 1.0, b.w - 2.0, b.h - 2.0, fill, 0);
        self.outline(b.x, b.y, b.w, b.h, BLACK);
        let label = fit(&b.label, b.w - 8.0);
        self.text_in(b.x + 4.0, b.y + 5.0, &label, ink, 1.0);
        let key = format!("({})", b.hotkey);
        let kw = font::width(&key) as f32;
        let cost = fit(&b.cost, b.w - kw - 10.0);
        self.text_in(b.x + 4.0, b.y + b.h - 11.0, &cost, ink, 1.0);
        self.text_in(b.x + b.w - kw - 4.0, b.y + b.h - 11.0, &key, ink, 1.0);
    }
}

/// Truncates text to what fits in `width` px at 1×.
fn fit(text: &str, width: f32) -> String {
    let max = (width / font::ADVANCE as f32).max(1.0) as usize;
    text.chars().take(max).collect()
}

/// "400F 200W": a cost in the resource bar's shorthand.
fn cost_label(cost: &Cost) -> String {
    let parts: Vec<String> = Resource::ALL
        .iter()
        .filter(|r| cost[r.index()] > 0)
        .map(|r| {
            format!(
                "{}{}",
                cost[r.index()],
                r.name().chars().next().unwrap_or('?').to_ascii_uppercase()
            )
        })
        .collect();
    parts.join(" ")
}

/// "400 FOOD 200 WOOD": a cost in full, for the tooltip line.
fn cost_words(cost: &Cost) -> String {
    let parts: Vec<String> = Resource::ALL
        .iter()
        .filter(|r| cost[r.index()] > 0)
        .map(|r| format!("{} {}", cost[r.index()], r.name().to_uppercase()))
        .collect();
    if parts.is_empty() {
        "FREE".to_string()
    } else {
        parts.join(" ")
    }
}

/// A short name that fits a button beside its cost.
fn short_name(kind: KindId) -> &'static str {
    match kind {
        kinds::HOUSE => "HOUSE",
        kinds::STOREHOUSE => "STORE",
        kinds::BARRACKS => "BARRACKS",
        kinds::FARM => "FARM",
        kinds::MARKET => "MARKET",
        kinds::ARCHERY_RANGE => "ARCHERY",
        kinds::STABLE => "STABLE",
        kinds::WATCH_TOWER => "TOWER",
        kinds::TEMPLE => "TEMPLE",
        kinds::ACADEMY => "ACADEMY",
        kinds::SIEGE_WORKSHOP => "SIEGE",
        kinds::GOVERNMENT_CENTRE => "GOVT",
        other => kinds::info(other).name,
    }
}

/// The key that places a building.
fn build_hotkey(kind: KindId) -> char {
    match kind {
        kinds::HOUSE => 'H',
        kinds::STOREHOUSE => 'O',
        kinds::BARRACKS => 'B',
        kinds::FARM => 'F',
        kinds::MARKET => 'M',
        kinds::ARCHERY_RANGE => 'N',
        kinds::STABLE => 'L',
        kinds::WATCH_TOWER => 'J',
        kinds::TEMPLE => 'P',
        kinds::ACADEMY => 'Y',
        kinds::SIEGE_WORKSHOP => 'G',
        kinds::GOVERNMENT_CENTRE => 'C',
        _ => 'N',
    }
}

/// The key that trains a unit. Keys repeat across buildings (the panel
/// only ever shows one building's roster) but never within one.
fn train_hotkey(kind: KindId) -> char {
    match kind {
        kinds::VILLAGER => 'V',
        kinds::CLUBMAN | kinds::AXEMAN | kinds::SCOUT => 'C',
        kinds::SPEARMAN => 'P',
        kinds::SLINGER => 'G',
        kinds::BOWMAN => 'B',
        kinds::LIGHT_CAVALRY => 'L',
        _ => 'N',
    }
}

/// A unit's name as a button fits it.
fn unit_label(kind: KindId) -> String {
    match kind {
        kinds::LIGHT_CAVALRY => "CAVALRY".to_string(),
        other => kinds::info(other).name.to_uppercase(),
    }
}

/// A unit's name in the plural, for the selection panel.
fn unit_plural(kind: KindId) -> String {
    match kind {
        kinds::CLUBMAN => "CLUBMEN".to_string(),
        kinds::AXEMAN => "AXEMEN".to_string(),
        kinds::SPEARMAN => "SPEARMEN".to_string(),
        kinds::BOWMAN => "BOWMEN".to_string(),
        kinds::LIGHT_CAVALRY => "LIGHT CAVALRY".to_string(),
        other => format!("{}S", kinds::info(other).name.to_uppercase()),
    }
}

/// The tooltip standard (`UX-TIP-01`): cost, time, what it does per hit,
/// what it counters and what counters it.
fn unit_tooltip(kind: KindId) -> String {
    let u = kinds::info(kind);
    let c = &u.combat;
    let mut t = format!(
        "{}: {}, {}S. {} HP",
        u.name.to_uppercase(),
        cost_words(&u.cost),
        u.build_seconds,
        u.max_health
    );
    if c.attack > 0 {
        t.push_str(&format!(
            ", {} {}",
            c.attack,
            c.damage.name().to_uppercase()
        ));
        if c.range > 0 {
            t.push_str(&format!(" RANGE {}", c.range));
        }
    }
    for (class, bonus) in c.bonuses {
        t.push_str(&format!(
            ". BONUS {bonus} VS {}",
            class.plural().to_uppercase()
        ));
    }
    let weak_to: Vec<String> = kinds::all()
        .iter()
        .filter(|k| k.combat.bonuses.iter().any(|(class, _)| *class == u.class))
        .map(|k| unit_plural(k.id))
        .collect();
    if !weak_to.is_empty() {
        t.push_str(&format!(". WEAK TO {}", weak_to.join(", ")));
    }
    t
}

fn stance_label(s: Stance) -> &'static str {
    match s {
        Stance::Aggressive => "AGGRESSIVE",
        Stance::Defensive => "DEFENSIVE",
        Stance::StandGround => "STAND",
        Stance::Passive => "PASSIVE",
    }
}

fn stance_tooltip(s: Stance) -> &'static str {
    match s {
        Stance::Aggressive => "AGGRESSIVE: CHASE ENEMIES IN SIGHT, THEN COME BACK",
        Stance::Defensive => "DEFENSIVE: FIGHT ENEMIES IN SIGHT, DO NOT CHASE FAR",
        Stance::StandGround => "STAND GROUND: FIGHT IN REACH, NEVER MOVE",
        Stance::Passive => "PASSIVE: NEVER FIGHT, RUN HOME WHEN HIT",
    }
}

fn formation_label(f: Formation) -> &'static str {
    match f {
        Formation::None => "NONE",
        Formation::Line => "LINE",
        Formation::Box => "BOX",
        Formation::Staggered => "STAGGER",
        Formation::Flank => "FLANK",
    }
}

fn formation_tooltip(f: Formation) -> &'static str {
    match f {
        Formation::None => "NO FORMATION: SPREAD OUT, EACH AT ITS OWN PACE",
        Formation::Line => "LINE: RANKS ABREAST, AT THE SLOWEST PACE",
        Formation::Box => "BOX: A SQUARE, AT THE SLOWEST PACE",
        Formation::Staggered => "STAGGERED: OPEN RANKS, AT THE SLOWEST PACE",
        Formation::Flank => "FLANK: TWO WINGS, AT THE SLOWEST PACE",
    }
}

/// Technology hotkeys, by position at the building.
const TECH_KEYS: [char; 5] = ['Q', 'E', 'I', 'K', 'Z'];

/// A button before it has a place on the grid.
struct Def {
    action: Action,
    label: String,
    cost: String,
    hotkey: char,
    enabled: bool,
    reason: String,
}

impl Def {
    fn on(
        action: Action,
        label: impl Into<String>,
        hotkey: char,
        reason: impl Into<String>,
    ) -> Def {
        Def {
            action,
            label: label.into(),
            cost: String::new(),
            hotkey,
            enabled: true,
            reason: reason.into(),
        }
    }

    fn costing(mut self, cost: &Cost) -> Def {
        self.cost = cost_label(cost);
        self
    }

    fn gated(mut self, check: Result<(), String>) -> Def {
        if let Err(why) = check {
            self.enabled = false;
            self.reason = why;
        }
        self
    }
}

/// The commands the selection offers, in grid order. Everything the player
/// could do from here is listed; what they cannot do yet is greyed with the
/// reason, so the panel visibly gains buttons as an age arrives.
fn commands(
    sim: &Simulation,
    me: u8,
    selected: &[Slot],
    build_mode: Option<KindId>,
    targeting: bool,
) -> Vec<Def> {
    let world = sim.world();
    let mut defs = Vec::new();
    if build_mode.is_some() {
        defs.push(Def::on(Action::Cancel, "CANCEL", 'X', "LEAVE PLACEMENT"));
        return defs;
    }
    if targeting {
        defs.push(Def::on(
            Action::Cancel,
            "CANCEL",
            'X',
            "CLICK THE GROUND TO GO THERE, OR CANCEL",
        ));
        return defs;
    }
    let Some(pl) = sim.player(me) else {
        return defs;
    };
    let own = |s: &Slot| world.owner[s.index()] == me;
    let any_villager = selected
        .iter()
        .any(|s| own(s) && world.kind[s.index()] == kinds::VILLAGER);
    let any_mobile = selected
        .iter()
        .any(|s| own(s) && kinds::info(world.kind[s.index()]).mobile);
    let building = selected.iter().copied().find(|s| {
        own(s)
            && !kinds::info(world.kind[s.index()]).mobile
            && world.construction[s.index()].is_none()
    });

    if any_mobile {
        defs.push(Def::on(
            Action::Stop,
            "STOP",
            'T',
            "STOP WHAT THEY ARE DOING",
        ));
    }
    let fighters: Vec<Slot> = selected
        .iter()
        .copied()
        .filter(|s| {
            own(s)
                && kinds::info(world.kind[s.index()]).mobile
                && kinds::info(world.kind[s.index()]).combat.attack > 0
        })
        .collect();
    if !fighters.is_empty() && !any_villager {
        // Soldiers only: villagers keep their building keys, and a mixed
        // selection is a villager selection with an escort.
        defs.push(Def::on(
            Action::AttackMove,
            "ATTACK MOVE",
            'M',
            "ADVANCE TO A POINT, FIGHTING ANYTHING ON THE WAY",
        ));
        defs.push(Def::on(
            Action::Patrol,
            "PATROL",
            'P',
            "WALK TO A POINT AND BACK, FIGHTING ANYTHING SEEN",
        ));
        let current = world.stance[fighters[0].index()];
        for (st, key) in Stance::ALL.iter().zip(['Q', 'E', 'I', 'K']) {
            // The current stance is bracketed: the font has no star.
            let label = if *st == current {
                format!("[{}]", stance_label(*st))
            } else {
                stance_label(*st).to_string()
            };
            defs.push(Def::on(
                Action::Stance(*st),
                label,
                key,
                stance_tooltip(*st),
            ));
        }
        let formation = world.formation[fighters[0].index()];
        defs.push(Def::on(
            Action::Formation(formation.next()),
            format!("FORM: {}", formation_label(formation)),
            'Z',
            format!(
                "{}. Z CYCLES: NEXT IS {}",
                formation_tooltip(formation),
                formation_label(formation.next())
            ),
        ));
    }
    if any_villager {
        let next = pl.age.next().unwrap_or(pl.age);
        // The Town Center is buildable in the table but not from a villager's
        // panel: in the design it comes with the Government Centre (M4+).
        let mut kinds_: Vec<&KindInfo> = kinds::all()
            .iter()
            .filter(|k| k.buildable && !k.mobile && k.id != kinds::TOWN_CENTER && k.age <= next)
            .collect();
        kinds_.sort_by_key(|k| (k.age, k.id));
        for k in kinds_ {
            let check = if k.age > pl.age {
                Err(format!("NEEDS THE {}", k.age.name().to_uppercase()))
            } else if !pl.can_afford(&k.cost) {
                Err("NOT ENOUGH RESOURCES".to_string())
            } else {
                Ok(())
            };
            defs.push(
                Def::on(
                    Action::Build(k.id),
                    short_name(k.id),
                    build_hotkey(k.id),
                    format!("{}: {}", k.name.to_uppercase(), cost_words(&k.cost)),
                )
                .costing(&k.cost)
                .gated(check),
            );
        }
    }
    if let Some(b) = building {
        let i = b.index();
        let id = world.id_at(b);
        let kind = world.kind[i];
        let info = kinds::info(kind);
        let queue_len = world.production[i].as_ref().map_or(0, |q| q.queue.len());
        if info.trains {
            // The roster, age-locked and unresearched lines greyed with
            // the reason, so the panel shows what is coming.
            for k in sim.roster(me, kind) {
                let u = kinds::info(k);
                let check = sim
                    .can_train(me, id, k)
                    .map_err(|e| e.to_string().to_uppercase());
                defs.push(
                    Def::on(
                        Action::Train(k),
                        unit_label(k),
                        train_hotkey(k),
                        unit_tooltip(k),
                    )
                    .costing(&u.cost)
                    .gated(check),
                );
            }
        }
        if kind == kinds::TOWN_CENTER {
            if let Some(t) = tech::age_advance(pl.age) {
                let check = sim
                    .can_research(me, id, t.id)
                    .map_err(|e| e.to_string().to_uppercase());
                defs.push(
                    Def::on(
                        Action::Research(t.id),
                        t.name.to_uppercase(),
                        'U',
                        format!("{}: {}", t.name.to_uppercase(), cost_words(&t.cost)),
                    )
                    .costing(&t.cost)
                    .gated(check),
                );
            }
        }
        let techs: Vec<&TechInfo> = tech::at_building(kind)
            .filter(|t| t.advances_age().is_none() && !pl.has_researched(t.id))
            .collect();
        for (n, t) in techs.iter().enumerate() {
            let check = sim
                .can_research(me, id, t.id)
                .map_err(|e| e.to_string().to_uppercase());
            defs.push(
                Def::on(
                    Action::Research(t.id),
                    t.name.to_uppercase(),
                    TECH_KEYS[n % TECH_KEYS.len()],
                    format!("{}: {}", t.name.to_uppercase(), cost_words(&t.cost)),
                )
                .costing(&t.cost)
                .gated(check),
            );
        }
        if matches!(kind, kinds::FARM | kinds::MARKET | kinds::TOWN_CENTER) {
            let label = if pl.auto_reseed {
                "RESEED ON"
            } else {
                "RESEED OFF"
            };
            defs.push(Def::on(
                Action::ToggleReseed,
                label,
                'R',
                format!(
                    "FARMS RESEED FOR {} WHEN EMPTY",
                    cost_words(&kinds::FARM_RESEED_COST)
                ),
            ));
        }
        if queue_len > 0 {
            defs.push(Def::on(
                Action::CancelTrain(id),
                "UNQUEUE",
                'X',
                "REMOVE THE LAST QUEUED ITEM AND REFUND IT",
            ));
        }
    }
    defs
}

/// True if a farm of `me` sits empty because the wood for reseeding is not
/// there ([GD-ECON-05]'s notification).
fn farm_needs_wood(sim: &Simulation, me: u8) -> bool {
    let world = sim.world();
    let Some(pl) = sim.player(me) else {
        return false;
    };
    pl.auto_reseed
        && !pl.can_afford(&kinds::FARM_RESEED_COST)
        && world.slots().any(|s| {
            let i = s.index();
            world.owner[i] == me
                && world.kind[i] == kinds::FARM
                && world.construction[i].is_none()
                && world.resource[i] <= 0
        })
}

/// "STONE AGE: 1/2 BUILDINGS FOR THE TOOL AGE", or the last age's name.
fn age_progress(sim: &Simulation, me: u8) -> String {
    let Some(pl) = sim.player(me) else {
        return String::new();
    };
    match (pl.age.next(), tech::age_advance(pl.age)) {
        (Some(next), Some(_)) => format!(
            "{}: {}/{} BUILDINGS FOR THE {}",
            pl.age.name().to_uppercase(),
            sim.age_buildings(me, pl.age),
            tech::AGE_BUILDINGS_REQUIRED,
            next.name().to_uppercase()
        ),
        _ => pl.age.name().to_uppercase(),
    }
}

/// One entry in the resource bar.
struct Seg {
    text: String,
    /// Shown small after the text: the worker count.
    sub: Option<String>,
    /// A box behind it, for warnings.
    boxed: Option<u8>,
}

/// The resource bar. Reflows rather than overlaps: at widths where the
/// large text and worker counts no longer fit beside the status, the counts
/// go, then the text shrinks, then the status goes.
fn top_bar(p: &mut Painter<'_>, sim: &Simulation, me: u8, vw: f32, status: &str, hint: bool) {
    p.rect(0.0, 0.0, vw, TOP_BAR, BROWN_DARK, 0);
    p.rect(0.0, TOP_BAR - 2.0, vw, 2.0, BLACK, 0);
    let world = sim.world();
    let mut segs: Vec<Seg> = Vec::new();
    if let Some(pl) = sim.player(me) {
        for r in Resource::ALL {
            let workers = world
                .slots()
                .filter(|s| {
                    world.owner[s.index()] == me
                        && matches!(world.order[s.index()], Order::Gather { resource, .. } if resource == r)
                })
                .count();
            segs.push(Seg {
                text: format!("{} {}", r.name().to_uppercase(), pl.stockpile[r.index()]),
                sub: Some(format!("({workers})")),
                boxed: None,
            });
        }
        let housed = pl.pop >= pl.pop_cap;
        segs.push(Seg {
            text: format!("POP {}/{}", pl.pop, pl.pop_cap),
            sub: None,
            boxed: housed.then_some(RED_DARK),
        });
        let idle = sim.idle_villagers(me).len();
        if idle > 0 {
            segs.push(Seg {
                text: format!("IDLE {idle}"),
                sub: None,
                boxed: Some(if idle > 3 { RED } else { GOLD_DARK }),
            });
        }
        segs.push(Seg {
            text: pl.age.name().to_uppercase(),
            sub: None,
            boxed: None,
        });
        if farm_needs_wood(sim, me) {
            segs.push(Seg {
                text: "FARM NEEDS WOOD".to_string(),
                sub: None,
                boxed: Some(RED_DARK),
            });
        }
        if hint {
            segs.push(Seg {
                text: "F1 CONTROLS".to_string(),
                sub: None,
                boxed: Some(GOLD_DARK),
            });
        }
    }
    let status_w = font::width(status) as f32 + 20.0;
    let measure = |scale: f32, subs: bool, gap: f32| -> f32 {
        10.0 + segs
            .iter()
            .map(|s| {
                let sub = match (&s.sub, subs) {
                    (Some(t), true) => font::width(t) as f32 + 4.0,
                    _ => 0.0,
                };
                font::width(&s.text) as f32 * scale + sub + gap
            })
            .sum::<f32>()
    };
    // (scale, worker counts, gap): the layouts in order of preference.
    let layouts = [
        (2.0, true, 40.0),
        (2.0, false, 24.0),
        (1.0, true, 16.0),
        (1.0, false, 12.0),
    ];
    let with_status = layouts
        .iter()
        .find(|(s, sub, g)| measure(*s, *sub, *g) + status_w <= vw);
    let (scale, subs, gap, show_status, two_lines) = match with_status {
        Some(&(s, sub, g)) => (s, sub, g, true, false),
        None => match layouts
            .iter()
            .find(|(s, sub, g)| measure(*s, *sub, *g) <= vw)
        {
            Some(&(s, sub, g)) => (s, sub, g, false, false),
            // Narrower than even the small layout: two lines, no status.
            None => (1.0, false, 12.0, false, true),
        },
    };
    let mut y = if two_lines {
        3.0
    } else if scale > 1.5 {
        6.0
    } else {
        10.0
    };
    let mut x = 10.0;
    for s in &segs {
        let w = font::width(&s.text) as f32 * scale;
        if two_lines && x + w > vw - 10.0 {
            if y > 3.0 {
                break; // Out of lines: the rest is dropped, not overlapped.
            }
            y = 14.0;
            x = 10.0;
        }
        if let Some(colour) = s.boxed {
            let pad = if two_lines { 1.0 } else { 3.0 };
            p.rect(
                x - 4.0,
                y - pad,
                w + 8.0,
                7.0 * scale + 2.0 * pad,
                colour,
                0,
            );
        }
        p.text(x, y, &s.text, false, scale);
        x += w;
        if let (Some(t), true) = (&s.sub, subs) {
            p.text(x + 4.0, y + 7.0 * scale - 7.0, t, false, 1.0);
            x += font::width(t) as f32 + 4.0;
        }
        x += gap;
    }
    if show_status {
        let sw = font::width(status) as f32;
        p.text(vw - sw - 10.0, 10.0, status, false, 1.0);
    }
}

impl Hud {
    /// Builds the HUD for a frame.
    pub fn build(atlas: &Atlas, input: &HudInput<'_>) -> Hud {
        let s = input.ui_scale.max(0.5);
        let scale = s;
        let (vw, vh) = (input.camera.viewport.0 / s, input.camera.viewport.1 / s);
        let hover = input.hover.map(|(x, y)| (x / s, y / s));
        let mut p = Painter::new(atlas);
        let mut buttons = Vec::new();
        let sim = input.sim;
        let world = sim.world();
        let me = input.player;

        // ----- top bar: resources, population, idle villagers, age, status
        let status = format!(
            "{}{} {:.0} FPS {:.1}X",
            input.status,
            if input.paused { " PAUSED" } else { "" },
            input.fps,
            input.speed
        );
        top_bar(
            &mut p,
            sim,
            me,
            vw,
            &status,
            !input.help && controls_hint(sim),
        );

        // ----- bottom panel
        let py = vh - BOTTOM_PANEL;
        p.rect(0.0, py, vw, BOTTOM_PANEL, BROWN_DARK, 0);
        p.rect(0.0, py, vw, 2.0, BLACK, 0);
        let reserve = MINIMAP_RESERVE.min(vw * 0.3);
        let panel_w = vw - reserve;
        let left_w = 260.0_f32.min(panel_w * 0.4).floor();
        p.rect(left_w, py + 8.0, 2.0, BOTTOM_PANEL - 16.0, BLACK, 0);

        // Selection summary.
        let selected: Vec<Slot> = world
            .slots()
            .filter(|s| input.selected.contains(&(s.index() as u32)))
            .collect();
        let mut ty = py + 10.0;
        let text_w = left_w - 20.0;
        match selected.len() {
            0 => {
                p.text(10.0, ty, "NOTHING SELECTED", false, 1.0);
                ty += 12.0;
                p.text(10.0, ty, "CLICK OR DRAG TO SELECT", false, 1.0);
            }
            1 => {
                let i = selected[0].index();
                let info = kinds::info(world.kind[i]);
                p.text(
                    10.0,
                    ty,
                    &fit(&info.name.to_uppercase(), text_w / 2.0),
                    false,
                    2.0,
                );
                ty += 20.0;
                let hp = fx_to_f32(world.health[i]);
                p.text(
                    10.0,
                    ty,
                    &format!("HP {:.0}/{}", hp, info.max_health),
                    false,
                    1.0,
                );
                // Health bar.
                let bw = text_w;
                p.rect(10.0, ty + 10.0, bw, 6.0, BLACK, 0);
                let frac = (hp / info.max_health as f32).clamp(0.0, 1.0);
                p.rect(
                    11.0,
                    ty + 11.0,
                    (bw - 2.0) * frac,
                    4.0,
                    if frac > 0.5 { GREEN_LIGHT } else { RED },
                    0,
                );
                ty += 22.0;
                if let Some(done) = world.construction[i] {
                    let pct = done * 100 / info.build_work().max(1);
                    p.text(10.0, ty, &format!("BUILDING {pct}%"), false, 1.0);
                    ty += 12.0;
                }
                if let Some((r, n)) = world.carry[i] {
                    p.text(
                        10.0,
                        ty,
                        &format!("CARRYING {} {}", n, r.name().to_uppercase()),
                        false,
                        1.0,
                    );
                    ty += 12.0;
                }
                if let Some((r, full)) = info.resource {
                    if !info.mobile && world.owner[i] != kinds::GAIA {
                        let full = sim.modifiers(world.owner[i]).farm_yield(full);
                        p.text(
                            10.0,
                            ty,
                            &format!("{} {}/{}", r.name().to_uppercase(), world.resource[i], full),
                            false,
                            1.0,
                        );
                        ty += 12.0;
                    }
                }
                if info.combat.attack > 0 && info.mobile {
                    let m = sim.modifiers(world.owner[i]);
                    let armour = crate::combat_view::armour_words(info, &m);
                    p.text(
                        10.0,
                        ty,
                        &fit(&crate::combat_view::attack_words(info, &m), text_w),
                        false,
                        1.0,
                    );
                    ty += 12.0;
                    p.text(10.0, ty, &fit(&armour, text_w), false, 1.0);
                    ty += 12.0;
                    if world.owner[i] == me {
                        let line = format!(
                            "{}, {}",
                            stance_label(world.stance[i]),
                            formation_label(world.formation[i])
                        );
                        p.text(10.0, ty, &fit(&line, text_w), false, 1.0);
                        ty += 12.0;
                    }
                }
                let job = match world.order[i] {
                    Order::Idle if info.mobile => "IDLE",
                    Order::Idle => "",
                    Order::Move { .. } => "MOVING",
                    Order::Gather {
                        phase: GatherPhase::Working,
                        ..
                    } => "GATHERING",
                    Order::Gather {
                        phase: GatherPhase::ToDropoff { .. },
                        ..
                    } => "RETURNING",
                    Order::Gather { .. } => "GOING TO GATHER",
                    Order::Build { working: true, .. } => "BUILDING",
                    Order::Build { .. } => "GOING TO BUILD",
                    Order::Attack { .. } => "ATTACKING",
                    Order::AttackMove { .. } => "ATTACK-MOVING",
                    Order::Patrol { .. } => "PATROLLING",
                    Order::Flee { .. } => "FLEEING",
                };
                if !job.is_empty() {
                    p.text(10.0, ty, job, false, 1.0);
                    ty += 12.0;
                }
                if let Some(q) = world.production[i].as_ref() {
                    if let Some(head) = q.queue.first() {
                        let (verb, name, total) = match head.item {
                            Item::Unit(k) => {
                                let u = kinds::info(k);
                                ("TRAINING", u.name.to_uppercase(), u.build_ticks())
                            }
                            Item::Tech(t) => {
                                let t = tech::info(t);
                                (
                                    "RESEARCHING",
                                    t.map_or("?", |t| t.name).to_uppercase(),
                                    t.map_or(1, |t| t.ticks()),
                                )
                            }
                        };
                        let pct = head.progress * 100 / total.max(1);
                        p.text(
                            10.0,
                            ty,
                            &fit(&format!("{verb} {name} {pct}%"), text_w),
                            false,
                            1.0,
                        );
                    }
                    // The queue strip: one box per item, the head filling
                    // as it progresses ([UX-CMD-06]).
                    let sy = py + BOTTOM_PANEL - 30.0;
                    for (n, item) in q.queue.iter().enumerate().take(5) {
                        let bx = 10.0 + n as f32 * 26.0;
                        p.rect(bx, sy, 22.0, 20.0, BLACK, 0);
                        p.rect(bx + 1.0, sy + 1.0, 20.0, 18.0, BROWN, 0);
                        let (letter, total) = match item.item {
                            Item::Unit(k) => {
                                let u = kinds::info(k);
                                (u.name.chars().next().unwrap_or('?'), u.build_ticks())
                            }
                            Item::Tech(t) => tech::info(t).map_or(('?', 1), |t| {
                                (t.name.chars().next().unwrap_or('?'), t.ticks())
                            }),
                        };
                        if n == 0 {
                            let frac = item.progress as f32 / total.max(1) as f32;
                            let fill = (18.0 * frac.clamp(0.0, 1.0)).round();
                            p.rect(bx + 1.0, sy + 19.0 - fill, 20.0, fill, GOLD, 0);
                        }
                        p.text(bx + 8.0, sy + 6.0, &letter.to_string(), true, 1.0);
                    }
                }
            }
            n => {
                let villagers = selected
                    .iter()
                    .filter(|s| world.kind[s.index()] == kinds::VILLAGER)
                    .count();
                p.text(10.0, ty, &format!("{n} SELECTED"), false, 2.0);
                ty += 20.0;
                if villagers > 0 {
                    p.text(10.0, ty, &format!("{villagers} VILLAGERS"), false, 1.0);
                    ty += 12.0;
                }
                // Soldiers by kind, in table order, as many as fit.
                let mut kinds_: Vec<KindId> = selected
                    .iter()
                    .map(|s| world.kind[s.index()])
                    .filter(|&k| k != kinds::VILLAGER && kinds::info(k).mobile)
                    .collect();
                kinds_.sort_unstable();
                kinds_.dedup();
                for k in kinds_.into_iter().take(4) {
                    let count = selected
                        .iter()
                        .filter(|s| world.kind[s.index()] == k)
                        .count();
                    p.text(
                        10.0,
                        ty,
                        &fit(&format!("{count} {}", unit_plural(k)), text_w),
                        false,
                        1.0,
                    );
                    ty += 12.0;
                }
            }
        }

        // Command grid: five across, three down, sized to the room it has.
        let grid_x = left_w + 14.0;
        let grid_w = (panel_w - grid_x - 6.0).max(BUTTON_MIN_W);
        let bw = ((grid_w - BUTTON_GAP * (GRID_COLS as f32 - 1.0)) / GRID_COLS as f32)
            .clamp(BUTTON_MIN_W, BUTTON_W)
            .floor();
        let defs = commands(sim, me, &selected, input.build_mode, input.targeting);
        for (n, d) in defs.into_iter().take(GRID_COLS * GRID_ROWS).enumerate() {
            let col = (n % GRID_COLS) as f32;
            let row = (n / GRID_COLS) as f32;
            let b = Button {
                x: grid_x + col * (bw + BUTTON_GAP),
                y: py + 10.0 + row * (BUTTON_H + BUTTON_GAP),
                w: bw,
                h: BUTTON_H,
                action: d.action,
                label: d.label,
                cost: d.cost,
                hotkey: d.hotkey,
                enabled: d.enabled,
                reason: d.reason,
            };
            let lit = hover.is_some_and(|(hx, hy)| b.contains(hx, hy));
            p.button(&b, lit);
            buttons.push(b);
        }
        // The line under the grid: the hovered button's story, else what
        // placement is doing, else where the player stands toward the next
        // age.
        let hovered = buttons
            .iter()
            .find(|b| hover.is_some_and(|(hx, hy)| b.contains(hx, hy)));
        let line = match (hovered, input.build_mode) {
            (Some(b), _) => b.reason.clone(),
            (None, Some(kind)) => format!(
                "PLACING {}: CLICK TO BUILD, ESC TO CANCEL",
                kinds::info(kind).name.to_uppercase()
            ),
            (None, None) => age_progress(sim, me),
        };
        let line_y = py + 10.0 + GRID_ROWS as f32 * (BUTTON_H + BUTTON_GAP) + 2.0;
        p.text(grid_x, line_y, &fit(&line, grid_w), false, 1.0);

        // Health bars over selected damaged units and construction bars over sites.
        for s in &selected {
            let i = s.index();
            let info = kinds::info(world.kind[i]);
            let hp = fx_to_f32(world.health[i]) / info.max_health as f32;
            let under_construction = world.construction[i].is_some();
            if hp >= 0.999 && !under_construction {
                continue;
            }
            let pos = world.pos[i];
            let (wx, wy) = (fx_to_f32(pos.x), fx_to_f32(pos.y));
            let (gx, gy) = iso::project(wx, wy, iso::ground_height(sim.map(), wx, wy));
            let (sx, sy) = input.camera.to_window(gx, gy);
            let (sx, sy) = (sx / scale, sy / scale);
            let lift = if info.footprint > 0 { 60.0 } else { 50.0 } * input.camera.zoom() / scale;
            let w = 32.0;
            p.rect(sx - w / 2.0 - 1.0, sy - lift - 1.0, w + 2.0, 6.0, BLACK, 0);
            let frac = if under_construction {
                world.construction[i].unwrap_or(0) as f32 / info.build_work().max(1) as f32
            } else {
                hp.clamp(0.0, 1.0)
            };
            let colour = if under_construction {
                GOLD
            } else if hp > 0.5 {
                GREEN_LIGHT
            } else {
                RED
            };
            p.rect(sx - w / 2.0, sy - lift, w * frac, 4.0, colour, 0);
        }

        // The age banner ([GD-AGE-02]): the moment gets the middle of the screen.
        if let Some(text) = input.banner {
            let scale = 3.0;
            let w = font::width(text) as f32 * scale + 32.0;
            let x = ((vw - w) / 2.0).round();
            let y = TOP_BAR + 28.0;
            p.rect(x, y, w, 40.0, BLACK, 0);
            p.rect(x + 2.0, y + 2.0, w - 4.0, 36.0, BROWN_DARK, 0);
            p.rect(x + 2.0, y + 2.0, w - 4.0, 2.0, GOLD, 0);
            p.rect(x + 2.0, y + 36.0, w - 4.0, 2.0, GOLD, 0);
            p.text_in(x + 16.0, y + 10.0, text, Ink::Gold, scale);
            let sub = "NEW BUILDINGS AND TECHNOLOGIES AVAILABLE";
            let sw = font::width(sub) as f32;
            p.text(((vw - sw) / 2.0).round(), y + 46.0, sub, false, 1.0);
        }

        // The controls overlay, over everything but the panels.
        if input.help {
            let [general, orders] = controls();
            let rows = general.len().max(orders.len());
            let pw = 620.0_f32.min(vw - 16.0);
            let line = 11.0;
            let ph = 40.0 + rows as f32 * line + 22.0;
            let world_h = vh - TOP_BAR - BOTTOM_PANEL;
            let x = ((vw - pw) / 2.0).round();
            let y = (TOP_BAR + ((world_h - ph) / 2.0).max(4.0)).round();
            p.rect(x, y, pw, ph, BLACK, 0);
            p.rect(x + 2.0, y + 2.0, pw - 4.0, ph - 4.0, BROWN_DARK, 0);
            p.rect(x + 2.0, y + 2.0, pw - 4.0, 2.0, GOLD, 0);
            p.rect(x + 2.0, y + ph - 4.0, pw - 4.0, 2.0, GOLD, 0);
            let title = "CONTROLS";
            let tw = font::width(title) as f32 * 2.0;
            p.text_in(
                x + ((pw - tw) / 2.0).round(),
                y + 10.0,
                title,
                Ink::Gold,
                2.0,
            );
            let col_w = (pw - 24.0) / 2.0;
            let key_w = 66.0;
            for (c, column) in [general, orders].iter().enumerate() {
                let cx = x + 12.0 + c as f32 * col_w;
                for (i, (key, what)) in column.iter().enumerate() {
                    let ly = y + 38.0 + i as f32 * line;
                    p.text_in(cx, ly, &fit(key, key_w - 6.0), Ink::Gold, 1.0);
                    p.text(cx + key_w, ly, &fit(what, col_w - key_w - 6.0), false, 1.0);
                }
            }
            let foot = "F1 OR ? CLOSES THIS";
            let fw = font::width(foot) as f32;
            p.text(
                x + ((pw - fw) / 2.0).round(),
                y + ph - 16.0,
                foot,
                false,
                1.0,
            );
        }

        // Everything above is in HUD pixels; the window wants device pixels.
        let mut sprites = p.out;
        if s != 1.0 {
            for sp in &mut sprites {
                sp.x *= s;
                sp.y *= s;
                sp.w *= s;
                sp.h *= s;
            }
            for b in &mut buttons {
                b.x *= s;
                b.y *= s;
                b.w *= s;
                b.h *= s;
            }
        }
        Hud { sprites, buttons }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::SimConfig;

    #[test]
    fn command_shortcuts_are_unique_and_do_not_use_camera_keys() {
        // These can coexist in mixed selections. Placement's Cancel uses
        // X in a separate panel, replacing all other buttons.
        let mut keys = std::collections::BTreeSet::from(['T', 'V', 'X', 'R', 'U']);
        for k in kinds::all()
            .iter()
            .filter(|k| k.buildable && k.id != kinds::TOWN_CENTER)
        {
            let key = build_hotkey(k.id);
            assert!(
                !"WASD".contains(key),
                "{} conflicts with camera movement",
                k.name
            );
            assert!(keys.insert(key), "duplicate build shortcut: {key}");
        }
        for key in TECH_KEYS {
            assert!(
                !"WASD".contains(key),
                "research conflicts with camera movement"
            );
            assert!(keys.insert(key), "duplicate research shortcut: {key}");
        }
        for k in kinds::all() {
            assert!(
                tech::at_building(k.id)
                    .filter(|t| t.advances_age().is_none())
                    .count()
                    <= TECH_KEYS.len()
            );
        }
    }

    /// Every unit tooltip carries cost, time, what it does per hit, what it
    /// counters and what counters it; every roster's keys are distinct.
    ///
    /// REQ: UX-TIP-01
    #[test]
    fn unit_tooltips_follow_the_standard_and_roster_keys_are_distinct() {
        let spear = unit_tooltip(kinds::SPEARMAN);
        assert!(
            spear.starts_with("SPEARMAN: 40 FOOD 20 WOOD, 26S. 45 HP, 4 MELEE"),
            "{spear}"
        );
        assert!(spear.contains("BONUS 6 VS CAVALRY"), "{spear}");
        assert!(spear.contains("WEAK TO SLINGERS"), "{spear}");
        let bow = unit_tooltip(kinds::BOWMAN);
        assert!(bow.contains("5 PIERCE RANGE 5"), "{bow}");
        assert!(
            !bow.contains("WEAK TO"),
            "nothing counters archers yet: {bow}"
        );
        let cav = unit_tooltip(kinds::LIGHT_CAVALRY);
        assert!(cav.contains("WEAK TO SPEARMEN"), "{cav}");
        let vill = unit_tooltip(kinds::VILLAGER);
        assert!(vill.contains("25 HP, 3 MELEE"), "{vill}");
        for b in kinds::all().iter().filter(|k| k.trains) {
            let keys: Vec<char> = kinds::trained_at(b.id)
                .filter(|u| tech::upgrade_of(u.id).is_none())
                .map(|u| train_hotkey(u.id))
                .collect();
            let mut unique = keys.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(keys.len(), unique.len(), "{}: {keys:?}", b.name);
            for u in kinds::trained_at(b.id) {
                assert_ne!(train_hotkey(u.id), 'N', "{}: has a key", u.name);
                assert!(!"WASD".contains(train_hotkey(u.id)), "{}: off WASD", u.name);
            }
        }
    }

    fn first_owned(sim: &Simulation, kind: KindId) -> u32 {
        sim.world()
            .slots()
            .find(|s| sim.world().kind[s.index()] == kind && sim.world().owner[s.index()] == 0)
            .unwrap()
            .index() as u32
    }

    /// The command grid gains its buttons as the age arrives, the queue strip
    /// shows what is in production, and the age banner goes up.
    ///
    /// REQ: GD-AGE-02
    /// REQ: UX-CMD-06
    #[test]
    fn hud_draws_resources_and_context_buttons() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let camera = Camera::new(sim.map().width(), sim.map().height(), (1280.0, 720.0));
        let villager = first_owned(&sim, kinds::VILLAGER);
        let tc = first_owned(&sim, kinds::TOWN_CENTER);
        let base = HudInput {
            sim: &sim,
            player: 0,
            camera: &camera,
            selected: &[],
            build_mode: None,
            fps: 60.0,
            paused: false,
            speed: 1.0,
            status: "T0",
            hover: None,
            banner: None,
            ui_scale: 1.0,
            help: false,
            targeting: false,
        };
        let none = Hud::build(&atlas, &base);
        assert!(none.buttons.is_empty());
        assert!(none.sprites.iter().all(|s| s.screen));
        assert!(none.sprites.len() > 40, "resource bar text");

        // A villager offers Stop, the Stone Age buildings, and the Tool Age
        // ones greyed with the reason.
        let v = Hud::build(
            &atlas,
            &HudInput {
                selected: &[villager],
                ..base
            },
        );
        let find =
            |hud: &Hud, action: Action| hud.buttons.iter().find(|b| b.action == action).cloned();
        assert_eq!(v.buttons[0].action, Action::Stop);
        let house = find(&v, Action::Build(kinds::HOUSE)).expect("house");
        assert!(house.enabled);
        assert_eq!(
            (house.label.as_str(), house.cost.as_str()),
            ("HOUSE", "30W")
        );
        assert!(
            find(&v, Action::Build(kinds::TOWN_CENTER)).is_none(),
            "a villager's panel does not offer a Town Center"
        );
        assert!(house.contains(house.x + 1.0, house.y + 1.0));
        assert!(find(&v, Action::Build(kinds::STOREHOUSE)).unwrap().enabled);
        let market = find(&v, Action::Build(kinds::MARKET)).expect("market is listed");
        assert!(!market.enabled);
        assert_eq!(market.reason, "NEEDS THE TOOL AGE");
        assert!(
            find(&v, Action::Build(kinds::SIEGE_WORKSHOP)).is_none(),
            "two ages ahead is not shown"
        );
        assert!(v.buttons.len() <= GRID_COLS * GRID_ROWS);

        // The Town Center trains, advances, and toggles reseeding.
        let t = Hud::build(
            &atlas,
            &HudInput {
                selected: &[tc],
                ..base
            },
        );
        assert!(find(&t, Action::Train(kinds::VILLAGER)).unwrap().enabled);
        assert!(find(&t, Action::Stop).is_none());
        let age = find(&t, Action::Research(tech::AGE_TOOL)).expect("age-up button");
        assert!(!age.enabled);
        assert_eq!(age.label, "TOOL AGE");
        assert!(age.reason.contains("BUILDINGS"), "{}", age.reason);
        assert_eq!(find(&t, Action::ToggleReseed).unwrap().label, "RESEED ON");
        assert!(
            !t.buttons
                .iter()
                .any(|b| matches!(b.action, Action::CancelTrain(_))),
            "nothing queued yet"
        );

        // Hovering a button puts its reason on the line under the grid.
        let hovered = Hud::build(
            &atlas,
            &HudInput {
                selected: &[tc],
                hover: Some((age.x + 2.0, age.y + 2.0)),
                ..base
            },
        );
        assert!(hovered.sprites.len() > t.sprites.len() - 40);

        let b = Hud::build(
            &atlas,
            &HudInput {
                selected: &[villager],
                build_mode: Some(kinds::HOUSE),
                ..base
            },
        );
        assert_eq!(b.buttons.len(), 1);
        assert_eq!(b.buttons[0].action, Action::Cancel);

        let banner = Hud::build(
            &atlas,
            &HudInput {
                banner: Some("TOOL AGE"),
                ..base
            },
        );
        assert!(banner.sprites.len() > none.sprites.len() + 8);
    }

    /// On a 2× display the same window is twice the device pixels; the HUD
    /// must come out at the same layout, twice the size, not at half size.
    #[test]
    fn the_hud_scales_with_the_display_instead_of_shrinking() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let villager = first_owned(&sim, kinds::VILLAGER);
        let one = Camera::new(sim.map().width(), sim.map().height(), (640.0, 360.0));
        let mut two = Camera::new(sim.map().width(), sim.map().height(), (1280.0, 720.0));
        two.dpi = 2.0;
        let build = |camera: &Camera, ui_scale: f32| {
            Hud::build(
                &atlas,
                &HudInput {
                    sim: &sim,
                    player: 0,
                    camera,
                    selected: &[villager],
                    build_mode: None,
                    fps: 60.0,
                    paused: false,
                    speed: 1.0,
                    status: "T0",
                    hover: None,
                    banner: None,
                    ui_scale,
                    help: false,
                    targeting: false,
                },
            )
        };
        let a = build(&one, 1.0);
        let b = build(&two, 2.0);
        assert_eq!(a.buttons.len(), b.buttons.len());
        assert_eq!(a.sprites.len(), b.sprites.len(), "same layout decisions");
        for (x, y) in a.buttons.iter().zip(&b.buttons) {
            assert_eq!(
                (y.x, y.y, y.w, y.h),
                (x.x * 2.0, x.y * 2.0, x.w * 2.0, x.h * 2.0)
            );
            assert_eq!(x.label, y.label);
        }
        for (x, y) in a.sprites.iter().zip(&b.sprites) {
            assert_eq!(
                (y.x, y.y, y.w, y.h),
                (x.x * 2.0, x.y * 2.0, x.w * 2.0, x.h * 2.0)
            );
        }
        // A hover given in device pixels lights the same button.
        let target = &b.buttons[1];
        let lit = Hud::build(
            &atlas,
            &HudInput {
                sim: &sim,
                player: 0,
                camera: &two,
                selected: &[villager],
                build_mode: None,
                fps: 60.0,
                paused: false,
                speed: 1.0,
                status: "T0",
                hover: Some((target.x + 2.0, target.y + 2.0)),
                banner: None,
                ui_scale: 2.0,
                help: false,
                targeting: false,
            },
        );
        assert_ne!(lit.sprites, b.sprites, "the hovered button draws lit");
    }

    /// The overlay lists every key the grid can hand out, and the hint that
    /// points at it lasts a minute.
    #[test]
    fn the_controls_overlay_covers_every_hotkey() {
        let [general, orders] = controls();
        let keys: Vec<String> = orders.iter().map(|(k, _)| k.clone()).collect();
        for k in kinds::all()
            .iter()
            .filter(|k| k.buildable && k.id != kinds::TOWN_CENTER)
        {
            assert!(
                keys.contains(&build_hotkey(k.id).to_string()),
                "{} has no line in the overlay",
                k.name
            );
        }
        for key in ["V", "U", "R", "X"] {
            assert!(keys.iter().any(|k| k == key), "{key} missing");
        }
        assert!(keys.iter().any(|k| k.contains('Q') && k.contains('Z')));
        let gen_keys: Vec<&str> = general.iter().map(|(k, _)| k.as_str()).collect();
        for key in ["WASD", "F2", "ESC", "SPACE", "T"] {
            assert!(gen_keys.contains(&key), "{key} missing");
        }

        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let camera = Camera::new(sim.map().width(), sim.map().height(), (960.0, 540.0));
        let base = HudInput {
            sim: &sim,
            player: 0,
            camera: &camera,
            selected: &[],
            build_mode: None,
            fps: 60.0,
            paused: false,
            speed: 1.0,
            status: "T0",
            hover: None,
            banner: None,
            ui_scale: 1.0,
            help: false,
            targeting: false,
        };
        let closed = Hud::build(&atlas, &base);
        let open = Hud::build(&atlas, &HudInput { help: true, ..base });
        assert!(
            open.sprites.len() > closed.sprites.len() + 200,
            "the overlay is drawn"
        );
        assert_eq!(open.buttons, closed.buttons, "the grid is untouched");
        assert!(controls_hint(&sim), "a fresh match shows the hint");
        let mut later = Simulation::new(
            5,
            SimConfig {
                map: sim::MapSpec {
                    kind: sim::MapKind::Flat,
                    size: 48,
                    players: 1,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        for _ in 0..HINT_TICKS {
            later.step();
        }
        assert!(!controls_hint(&later), "and drops it after a minute");
    }

    #[test]
    fn the_resource_bar_reflows_instead_of_overlapping() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        for width in [1280.0, 960.0, 640.0, 400.0] {
            let camera = Camera::new(sim.map().width(), sim.map().height(), (width, 360.0));
            let hud = Hud::build(
                &atlas,
                &HudInput {
                    sim: &sim,
                    player: 0,
                    camera: &camera,
                    selected: &[],
                    build_mode: None,
                    fps: 60.0,
                    paused: false,
                    speed: 1.0,
                    status: "SEED 5 TICK 0",
                    hover: None,
                    banner: None,
                    ui_scale: 1.0,
                    help: false,
                    targeting: false,
                },
            );
            // Every glyph in the top bar stays inside the window.
            let glyphs: Vec<&SpriteInstance> = hud
                .sprites
                .iter()
                .filter(|s| s.y < TOP_BAR && s.uw == font::GLYPH_W as u16)
                .collect();
            assert!(!glyphs.is_empty());
            for g in &glyphs {
                assert!(
                    g.x >= 0.0 && g.x + g.w <= width,
                    "glyph off screen at {width}px"
                );
            }
            // And no two glyph rows collide: text is laid out left to right
            // with a gap, so sort by x and check each starts after the last
            // one on the same line ends.
            let mut rows: Vec<(i32, f32, f32)> = glyphs
                .iter()
                .map(|g| (g.y as i32, g.x, g.x + g.w))
                .collect();
            rows.sort_by(|a, b| (a.0, a.1).partial_cmp(&(b.0, b.1)).unwrap());
            for pair in rows.windows(2) {
                if pair[0].0 == pair[1].0 {
                    assert!(
                        pair[1].1 >= pair[0].2 - 0.01,
                        "glyphs overlap at {width}px: {:?} {:?}",
                        pair[0],
                        pair[1]
                    );
                }
            }
        }
    }
}
