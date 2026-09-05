# New Empire

A real-time strategy game about taking a civilization from hand-axes to iron in
about half an hour — built to recapture what made *Age of Empires* (1997)
engaging, without its 1997 frustrations.

**Status: specification. No code yet.**

---

## The specification

Read in order:

| Document | What it covers |
|---|---|
| [01 — Research: Age of Empires (1997)](docs/01-research-age-of-empires.md) | What the original did, how it ran, what it got wrong, and the actual mechanisms behind why it was engaging |
| [02 — Game design spec](docs/02-game-design-spec.md) | Pillars, resources, ages, units, buildings, tech, combat, maps, victory, civilizations, AI |
| [03 — UX and feel spec](docs/03-ux-and-feel-spec.md) | Screen layout, selection, commands, camera, feedback, audio design, onboarding |
| [04 — Technical architecture](docs/04-technical-architecture.md) | Rust workspace, deterministic simulation, tick model, entity store, pathfinding, rendering, testing |
| [05 — Art and audio spec](docs/05-art-and-audio-spec.md) | Isometric projection, sprite and animation standards, palette, terrain, UI art, audio inventory |
| [06 — Roadmap](docs/06-roadmap.md) | M0–M9 milestones with demonstrable acceptance criteria |
| [07 — Decisions and open questions](docs/07-decisions-and-open-questions.md) | Decision log with reasoning, and what still needs answering |

## Design pillars

1. **You can see your empire advance** — progress is expressed in the world, not in a progress bar.
2. **The map is a finite, shared board** — resources deplete and never return; economic pressure creates conflict.
3. **Commands are cheap; micromanagement is optional** — a relaxed player and a fast player both get a good match.
4. **Everything is legible, and everything makes a sound.**
5. **The simulation is deterministic** — replays, saves and future multiplayer all follow from this.

## Shape of the build

- **Rust**, native desktop (Windows / macOS / Linux), `wgpu` + `winit` + `kira`.
  The same stack compiles to WebAssembly for quick playtest builds.
- **Deterministic lockstep simulation** at 20 Hz, fixed-point maths, seeded RNG,
  commands scheduled two ticks ahead — the architecture from *"1500 Archers on a
  28.8"*, which is what let the original run 1,500 units over a modem.
- **2D isometric sprites**, 64×32 tiles, 8 facings authored as 5 and mirrored,
  palette-indexed with a reserved player-colour ramp — Genie engine technique,
  our own art.

## First playable target (M7)

One random map type, two civilizations, Stone → Tool → Bronze, ~12 unit types,
skirmish against an AI opponent, with the full audio and animation pass. A
complete 25-minute match, start to victory screen.

## Assets and legal position

No Age of Empires assets, data files or code are used anywhere in this project.
All art, audio and balance data are original. The techniques documented in
`docs/01` (isometric tiling, palette-indexed player colours, sprite mirroring,
deterministic lockstep networking) are published engineering practice and are
what we are building on.
