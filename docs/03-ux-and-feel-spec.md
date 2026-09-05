# UX and Feel Spec

The user experience is the point of this project. The systems in
`docs/02-game-design-spec.md` are only the substrate; what people remember about
*Age of Empires* is how it **felt** to click on things. This document specifies
that, at implementation detail.

---

## 1. Screen layout

Reference layout at 1920×1080; everything scales proportionally and reflows for
ultrawide.

```
┌──────────────────────────────────────────────────────────────────────┐
│ 🍖 640  🌲 310  ⛏ 120  🪙 85     Pop 32/40   [!]Idle: 2   Age: Tool  │  ← Resource bar (top, 44px)
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│                                                                      │
│                        ISOMETRIC WORLD VIEW                          │
│                    (fills all remaining space)                       │
│                                                                      │
│   ┌────────────────┐                                                 │
│   │ notifications  │                                                 │
│   └────────────────┘                                                 │
├───────────────┬──────────────────────────────────┬───────────────────┤
│  Selection    │      Command grid (5×3)          │                   │
│  portrait     │  ┌──┬──┬──┬──┬──┐                │     MINIMAP       │
│  + stats      │  ├──┼──┼──┼──┼──┤                │   (bottom right)  │
│  + unit icons │  └──┴──┴──┴──┴──┘                │                   │
└───────────────┴──────────────────────────────────┴───────────────────┘
   ← Command panel (bottom, ~180px)
```

**Rules:**

- **Resource bar top, command panel bottom, minimap bottom-right.** This is the
  layout the game we are evoking used, and it is muscle memory for the audience.
  We are not innovating here.
- The resource bar shows, per resource: current stockpile **and the number of
  villagers currently assigned to it**. The original showed this; most modern
  RTS games do not; it is genuinely useful.
- **Idle villager counter** in the resource bar, always visible, clickable,
  turning amber above 3 and red above 6.
- The world view is never occluded by a modal during play. Menus pause (single
  player) or overlay translucently (multiplayer).

---

## 2. Selection

| Input | Behaviour |
|---|---|
| Left click | Select one unit or building |
| Left drag | Band-box select — **no unit cap** |
| Double click | Select all visible units of that type on screen |
| Ctrl + double click | Select all units of that type on the map |
| Shift + click | Add/remove from selection |
| Ctrl + 1–9 | Assign control group |
| 1–9 | Select control group; press twice to centre camera on it |
| Shift + 1–9 | Add group to selection |
| Tab | Cycle sub-groups within a mixed selection |
| `.` | Select next idle villager |
| `,` | Select next idle military unit |
| Ctrl + A | Select all military units |
| H | Centre on Town Center |
| Esc | Clear selection / cancel current command |

**Selection ordering is stable.** Repeated band-boxes of the same units yield
the same order, so the selection panel does not shuffle.

Selected units get a coloured ellipse in their player colour, plus a health bar
above (always on for damaged units, toggleable to always-on for all).

---

## 3. Commands

**Right-click is contextual**, and the cursor tells you what it will do before
you click:

| Target under cursor | Action | Cursor |
|---|---|---|
| Ground | Move | Arrow |
| Resource | Gather that resource | Resource glyph |
| Own damaged building/unit | Repair / heal | Wrench / cross |
| Own transport or garrisonable building | Garrison | Door |
| Enemy unit or building | Attack | Sword |
| Enemy unit, priest selected | Convert | Chant glyph |
| Minimap | Move to that world location | Same as above |

The full command vocabulary — all of it absent from the 1997 original, all of it
non-negotiable now:

- **Attack-move** (`A` then click): advance, engaging anything on the way.
- **Patrol** (`P`): move back and forth, engaging.
- **Waypoints** (Shift + click): queue any sequence of commands, including
  mixed types — move here, build this, then gather that.
- **Rally points**, including **onto a resource** (new villagers walk out and
  start gathering it) or onto a unit (they follow it).
- **Production queue** — click to queue one, Shift+click to queue five, with a
  visible queue strip and refund on cancel.
- **Stances** — aggressive / defensive / stand ground / passive, per unit,
  settable on a selection.
- **Formations** — line, box, staggered, flank. Units keep formation while
  moving and the group moves at the speed of its slowest member (toggleable).
- **Garrison / ungarrison** for towers, Town Centers and transports.
- **Delete** (Del, with confirmation for buildings).

**A queued command is always shown**: waypoint flags on the ground, a dotted line
between them, and a ghost of the queued building.

---

## 4. Camera

- Edge scroll (with a configurable dead zone and off switch), `WASD` / arrows,
  and middle-mouse drag.
- **Discrete zoom levels only** — 1×, 1.5×, 2× — so pixel art stays crisp. No
  free-scroll zoom; it makes sprite art look bad.
- Minimap click to jump, drag to scrub.
- Camera speed is user-configurable and frame-rate independent.
- Alt + click on a notification jumps the camera to the event.

---

## 5. Building placement

The placement interaction from the original, cleaned up:

- Holding a building ghost snaps to the tile grid, tinted **green when valid,
  red when blocked**, with the blocking tiles individually highlighted.
- Foundation footprint is shown as a grid overlay, plus the building's
  **influence radius** where relevant (tower range, drop-off distance ring,
  house pop contribution).
- **Shift keeps the ghost active** for repeat placement (walls, houses, farms).
- **Wall dragging**: click-drag places a continuous run of wall segments, with a
  live cost readout and automatic gate suggestion at road crossings.
- A **drop-off distance heat overlay** (toggle key `V`) shows walking cost from
  resources to your nearest Storehouse. This is a teaching tool that makes an
  invisible skill visible.

---

## 6. Feedback — the part that actually creates the feeling

### 6.1 Audio

Audio is not decoration here; it is the primary feedback channel. Four buses:

1. **UI** — clicks, invalid-action buzz, menu.
2. **Unit acknowledgment** — the bark when you order a unit. Every unit type has
   3–5 variations, randomised, never twice in a row. This is the single most
   important sound in the game: it is what makes units feel like they *heard you*.
3. **World SFX (positional)** — axes on wood, picks on stone, sheep, arrows,
   building construction, collapse. Volume and pan derived from screen position;
   attenuated off-screen but not silenced, so you hear your economy running.
4. **Music** — one track per age, cross-fading on age-up, plus a combat layer
   that ducks in when a fight starts near the camera.

Rules:

- Every player action gets a sound within **50 ms**. No exceptions.
- Concurrent identical sounds are voice-limited and slightly pitch-varied so a
  woodline of twelve villagers is a texture, not a machine gun.
- Positional world audio uses the camera centre as the listener.
- Ambient beds per terrain type (forest birds, coastal surf, desert wind) at
  low volume.

### 6.2 Visual feedback

- **Hit reaction**: a small flinch and impact spark on every hit; a directional
  blood/dust puff on kill.
- **Death animations** per unit type, then a corpse that fades over ~30 seconds.
  Corpses are visual only, no sim cost beyond a timer.
- **Building destruction**: collapse animation, dust cloud, and rubble that
  persists for 60 seconds.
- **Construction**: buildings rise in three visible stages (foundation → frame →
  complete), with villager hammering animations and dust.
- **Resource depletion is visible**: berry bushes thin out, gold veins shrink,
  trees fall in the direction the villager was standing.
- **Damage direction indicator** on the screen edge when off-screen units of
  yours are attacked.

### 6.3 Notifications

A stack in the lower-left, each with an icon, a spoken/played cue and a
click-to-jump:

| Event | Notification |
|---|---|
| Your town is under attack | Bell toll + minimap flash + "Your town is under attack" |
| A unit or building of yours destroyed | Subtle thud, minimap ping |
| Research or age-up complete | Fanfare, panel highlight |
| A new unit is idle at a full rally | Soft chime |
| Cannot afford / population capped | Distinct voice line, plus the resource in the bar flashing |
| Enemy Wonder started | Global announcement, permanent minimap marker |

Attack notifications are rate-limited (one per area per 20 seconds) so a long
siege does not become an alarm loop.

---

## 7. Onboarding

The original taught through a campaign, and it worked. We do the same:

- **Scenario 1 teaches gathering and building. Scenario 2 teaches age-up.
  Scenario 3 teaches combat. Scenario 4 teaches counters.** One idea each.
- Contextual first-time hints ("Villagers are idle — press `.` to find them"),
  each shown at most twice, all disableable.
- A **tooltip standard**: every unit and building tooltip shows cost, build time,
  what it counters, what counters it, and the hotkey. Tooltips are the manual.
- No wall of text anywhere. If a concept needs a paragraph, the design is wrong.

---

## 8. What we are explicitly fixing from 1997

| Original frustration | Our behaviour |
|---|---|
| Units get stuck on each other | Units push through idle friendlies; blocked units repath immediately |
| Faster units cannot overtake slower ones | Movement resolves per unit; formation speed matching is opt-in |
| Villagers stand still and die | Passive stance: flee toward the Town Center and raise an alarm |
| Selection capped at ~25 | Uncapped |
| No unit queueing | Full queue with Shift+click for five |
| Farms expire silently | Auto-reseed with a wood-shortage notification |
| No attack-move, patrol, waypoints, formations | All present, bound by default |
| Finding idle villagers is manual | Persistent counter plus `.` cycling |
| Cannot tell why an action failed | Every refused action gets a specific reason, spoken and written |

---

## 9. Performance targets that are UX targets

These are experience requirements, not engineering nice-to-haves:

- **60 fps sustained** with 400 units on screen; never below 30 fps at the
  200-population cap.
- **Command latency under 100 ms** from click to visible unit response.
- **Cold start to main menu under 3 seconds**; skirmish load under 5 seconds.
- **Camera scroll is frame-perfect smooth** — a stuttering camera reads as a
  broken game faster than almost any other defect.
