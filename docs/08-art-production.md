# Art Production

Where the art comes from, what it costs, and why the pipeline is shaped the way
it is. This answers Q6 in `docs/07`, which was blocking M3.

`docs/05-art-and-audio-spec.md` says what the art must *be*. This says how it
gets made.

The constraint that drives everything here: it has to feel like the art of the
late-90s isometric RTS, and none of it may be copied from one.

---

## 1. The size of the problem

The frame count is not a guess. It falls out of the animation table in
`docs/05` §2.2 multiplied by the five authored facings in §2.1:

```
idle 4 + walk 8 + attack 6 + death 8 + decay 4  =  30 frames
30 frames × 5 authored facings                  = 150 frames
```

**150 frames per mobile unit, per age costume.** A villager also carries eight
task animations and four carry variants, which is 550 frames for a villager in
a *single* age.

Buildings are cheaper per unit but multiply differently: `docs/05` §2.5 requires
one visibly different structure per age a building exists in — not a recolour —
and each of those needs three construction stages and a rubble state on top of
the finished sprite. A Town Center exists in all four ages, so it is 4 × 5 = 20
images, doubled again if the two civilizations do not share an architecture set.

Running the whole inventory in `docs/02` §5 and §6 through that:

| | Unit frames | Building images | Terrain | Icons | **Total** |
|---|---|---|---|---|---|
| **Vertical slice (M7)** — 12 units, Stone/Tool/Bronze, 10 buildings, 2 architecture sets | 3,900 | 230 | 184 | 51 | **~4,400** |
| **Full game** — 22 units, 19 buildings, 4 ages | 7,300 | 530 | 184 | 96 | **~8,100** |

Terrain is 8 types × 4 variants, plus 8 directional blend masks per adjoining
pair, plus 12 cliff tiles per elevation transition, plus clutter. Icons are one
per unit, building, technology and command at 48 × 48.

These numbers are the argument for everything below. Four thousand images is
not a "commission some sprites" problem.

---

## 2. The part that is actually hard

Every one of those images has to agree with every other one on six things:

1. **Light direction** — one key light, high and to the left. One frame lit from
   the right and the unit looks like it teleported.
2. **Palette** — the same 256 indices, with player colour only in 240–247.
3. **Anchor** — the ground contact point, to the pixel. `docs/05` §2.3 calls
   getting this wrong "the most common source of sprites sliding around".
4. **Proportion** — a villager is a fixed number of pixels tall in every frame
   of every animation.
5. **Costume detail** — the same belt, in the same place, in all 150 frames.
6. **Identity across angles and time** — the same person, recognisably, from
   five directions across thirty frames.

The first five are discipline, and the atlas tool enforces them mechanically.
The sixth is the one that decides which production route is viable, because
**generative image models have no persistent representation of the subject
between images.** Rotational and temporal coherence is precisely their weakest
axis. Tools that add skeletal rigging and n-directional rotation are attempts to
paper over exactly this, and they work per character, not per project: the
failure mode is drift, and drift across 4,400 images is not something you can
review your way out of.

---

## 3. How the original was actually made

Worth knowing before choosing, because it reframes the problem.

*Age of Empires* (1997) did not draw its sprites. Every unit and animation was
modelled and animated in 3D Studio Max and converted to 2D, explicitly to save
time and hold consistency ([Genie Engine][genie], [DOS Days][dosdays]).

So the look we are chasing is not a drawing style that belongs to anyone. It is
the residue of a *pipeline*: a fixed 2:1 dimetric camera, one hard key light, a
render, a downsample, and a quantisation to a few hundred indexed colours. The
chunky readable silhouettes, the slightly soft dense shading, the low frame
counts with one hard impact frame — those are all artefacts of that process.

This has a direct bearing on the brief. Prompting a generative model for "Age of
Empires style" is the route that pulls hardest *toward* Microsoft's actual
assets, because that phrase is a pointer at their art in the model's training
data. Reproducing the *method* that produced the look reaches the same
destination and cannot copy anything, because there is nothing to copy from —
the output is a render of geometry we built.

---

## 4. Options considered

### 4.1 Direct AI sprite generation

The purpose-built tools are real, cheap and much better than general-purpose
image models at this.

[PixelLab][pixellab] is the strongest on paper: an HTTP API with 8-directional
character rotation, skeleton-based animation, isometric tilesets, forced
palettes and style-reference images. Per-generation costs are trivial — roughly
$0.017 for an 8-direction character at 64 × 64, about $0.10 for 256 × 256 work —
so the entire slice inventory is tens of dollars of compute. Character and
rotation work caps at 168 × 168, which is comfortably above our largest mobile
frame (a villager at 2× is 80 × 96). Isometric tileset generation caps at 32 ×
32, which is below our 128 × 64 authoring tile, so terrain would not come from
there.

[Retro Diffusion][retrodiffusion] is the better provenance story: it was trained
on its creator's own catalogue plus work from artists who consented, rather than
on a scrape, and [its terms leave output ownership with whoever generated
it][rdterms].

**Why not this as the primary route.** Cost is not the constraint; coherence is.
Nothing in this class solves item 6 in §2 across an inventory this size, and the
copyright position in §5.1 means the output would be unprotectable.

### 4.2 Permissively licensed asset packs

Kenney's 40,000+ CC0 assets and [OpenGameArt's CC0 isometric collections][ogacc0]
are genuinely usable commercially, with no attribution required. But nothing
available is an eight-facing animated ancient-world unit set with per-age costume
variants, and the styles on offer are clean 3D-rendered chunk rather than 1997
pixel. Useful for greybox and for testing the pipeline. Not shippable art.

### 4.3 Commissioning

Highest quality ceiling and the cleanest rights position — a work-for-hire
contract gives us assignable copyright in everything. Also the slowest and by
far the most expensive way to produce 4,400 images, and it puts the vertical
slice's schedule inside someone else's calendar.

Still the right answer for a small number of things where quality per image
matters most and volume is low: the ~96 icons, the UI panel set, and the Wonder.

### 4.4 Render-to-sprite — **chosen**

Model and rig the subject once; render the frames.

You author roughly **24 rigged models** instead of 8,100 frames. Everything in
§2 that has to stay consistent is consistent by construction, because it is the
same geometry, the same camera and the same light every time. Age variants —
which `docs/05` §2.5 calls "the single largest art cost in the project" —
collapse from a re-draw to a costume mesh swap and a re-render. And the frames
are reproducible: a fixed camera and a fixed light mean re-rendering next year
produces the same pixels, so the art can be regenerated rather than archived.

Generative AI still earns its place inside this route, just upstream of the
frames: concept silhouettes and costume exploration, texture generation for the
models, image-to-3D for base meshes to kitbash from, and the icons, which are
single static images with no coherence problem at all.

---

## 5. The legal position

### 5.1 Copyright

The US Copyright Office holds that copyright does not extend to purely
AI-generated material and that "prompts do not alone provide sufficient
control"; the human-authorship requirement was settled by the Supreme Court's
denial of certiorari in March 2026 ([copyright.gov/ai][usco]).

The consequence for us is concrete and is easy to miss: **prompt-generated
sprites would be unprotectable, so anyone could lift them out of our data files
and ship them.** Modelled and rendered art is human-authored and protectable in
the ordinary way. This is a second, independent reason to prefer §4.4 that has
nothing to do with quality.

### 5.2 Platform disclosure

Steam's policy, as revised in January 2026, requires disclosure of AI-generated
content that players consume — art, audio, dialogue, localisation, marketing —
and [explicitly exempts tools used for development efficiency and for concept
ideation whose output does not ship][pcgamer].

Under §4.4 the shipped pixels are renders of geometry we made. AI used for
concept exploration upstream falls inside the ideation exemption. We should
still expect to answer the question honestly at submission, and the answer
should be written down before then, not improvised.

### 5.3 Provenance

Nothing in the pipeline ingests Age of Empires assets, data files or code, which
is the position `README.md` already states. The techniques — isometric tiling,
palette-indexed player colour, sprite mirroring, deterministic lockstep — are
published engineering practice.

One rule follows from §3 and belongs in the brief for anyone working on this:
**do not prompt any generative tool with the name of a shipped game, studio or
franchise.** Describe the subject and the era. "Bronze age spearman, hide
tunic, bronze helmet" is a description of history. The other kind of prompt is
a request for someone's assets.

---

## 6. The palette

`assets/palette/ancient.ron` is the source of truth, and `atlas` expands it into
the 256 entries the renderer uploads. Ramps are authored as two endpoints plus a
hue rotation rather than as 216 hand-picked values, because endpoints are what
an artist reasons about, and a ramp interpolated in Oklab lands where a
hand-picked one would. The hue shift bows through the midtones and returns to
the authored colour at both ends, which is the pixel-art ramp technique that
makes shading read as light having a colour.

Index layout is a contract:

| Indices | Contents |
|---|---|
| 0 | Transparent |
| 1–16 | Neutral ramp — outlines, shadows, greys, UI |
| 17–232 | 27 material ramps, 8 steps each |
| 233–239 | Reserve, currently the error colour so accidental use is loud |
| 240–247 | **Player colour**, remapped per owner in the fragment shader |
| 248–255 | Selection, health, fog, UI accent, outline, error |

### Player colours are a measured result, not a taste decision

`docs/05` §2.4 requires the eight player colours to be "checked against
deuteranopia and protanopia simulation before we commit". That check is now a
test rather than an intention: `atlas` simulates both dichromacies after Viénot,
Brettel and Mollon (1999) and measures the smallest Oklab distance between any
two owners across the body of their ramps.

Hue alone cannot separate eight owners for a dichromat — red, green, yellow and
orange collapse toward one axis and blue, cyan and magenta toward the other. The
ramps were therefore searched rather than picked: hue pinned near each colour's
name so it still answers to it, then lightness and chroma optimised to maximise
the worst pair. Measured results:

| Vision | Worst pair | Separation |
|---|---|---|
| Normal | green vs cyan | 0.086 |
| Protanopia | blue vs magenta | 0.076 |
| Deuteranopia | green vs orange | 0.072 |

The bars in the palette file are set just below these, so the test is a
regression guard: an edit that closes a gap fails `cargo test`.

**0.072 is not comfortable, and it is close to the ceiling for eight colours
that still answer to their names.** Two mitigations follow, and the first is
already in place:

- Owners are assigned in palette order, so a 1v1 is blue against red and a
  four-player game never reaches cyan. The first four are held to a much higher
  bar (0.15 measured 0.160) than the back four.
- The real fix is an ownership cue that is not colour at all. That is Q9 in
  `docs/07`.

---

## 7. The pipeline

```
   concept  ──▶  model + rig  ──▶  render  ──▶  atlas quantize  ──▶  atlas validate  ──▶  game
  (AI ok)       (Blender)        (5 azimuths,   (downsample,        (size, anchors,
                                  fixed light)   index, player       facings, palette)
                                                 colour key)
```

**Camera.** One orthographic camera at the 2:1 dimetric angle that
`docs/05` §1's projection implies — 30° above the horizon, orbited to the five
authored azimuths (S, SW, W, NW, N). The renderer mirrors those for SE, E and
NE, so the model must not carry an asymmetric detail that would flip.

**Light.** One key light, high and to the left, plus a weak fill. Fixed for
every subject in the game. This is the single most important thing to freeze
early — it is what makes art from different sources cut together, and changing
it later invalidates every rendered frame.

**Render size.** Render at 2× the authoring size, i.e. 4× the 1× sprite, and
box-filter down in linear light. Rendering straight to the authoring size gives
ragged edges; the supersample is what produces the soft dense look of a
pre-rendered sprite.

**Player colour.** A render has no notion of palette indices, so surfaces meant
to take player colour are textured in a key hue — pure magenta, which appears
nowhere in the palette. `atlas quantize` maps pixels near that hue into indices
240–247 *by lightness*, so the renderer's shading survives into the ramp. Every
other pixel matches to the nearest ordinary index, and quantisation is
explicitly forbidden from landing in the player range, or parts of a sprite
would recolour themselves per owner.

**Validation.** `atlas validate` is the gate `docs/05` §6 requires. It checks
frame size against the class table, sheet dimensions against the manifest,
palette identity, transparency, anchors against each frame's drawn content,
facing counts, animation names and frame counts, the impact frame on attacks,
and that every unit and building actually uses some player colour — a sprite
with none has an invisible owner. It reports every violation rather than the
first, and it exits non-zero, so CI fails.

---

## 8. What exists now

`tools/atlas`, run by `scripts/check-art.sh` in CI:

```sh
cargo run -p atlas -- palette      # bake, and report player-colour separation
cargo run -p atlas -- export       # swatch PNG + .gpl for Aseprite/GIMP/Krita
cargo run -p atlas -- placeholder  # generate the placeholder catalogue
cargo run -p atlas -- validate     # the conformance gate
cargo run -p atlas -- quantize --in render.png --out sprite.png
```

The **placeholder pass** (`docs/05` §6 step 1) is done: 26 sprite sets covering
the vertical-slice units, buildings and terrain, with correct sizes, anchors,
five facings, the full animation set, and player colour in the reserved ramp.

They are generated as *files* rather than at runtime on purpose. Runtime shapes
prove the renderer can draw a diamond; files prove the whole pipeline —
manifest, sheet layout, indexed palette, player-colour remapping, anchors,
facings — which is step 2 of the same production plan. The placeholders pass
through `atlas validate` exactly as real art will, so the gate is load-bearing
from M1 rather than from the first drawn sprite. They are also deliberately
readable: different silhouettes, heights and accent colours per unit, and a
facing mark that distinguishes all five directions, so a playtest can tell a
bowman from a clubman before any art exists.

Generated art is not committed. It is derived from the palette, so a committed
copy could only go stale; CI regenerates it and validates the result, which also
proves a fresh clone can produce what the game loads.

## 9. What comes next

1. **Freeze the camera and light rig** as a Blender file under `tools/render/`,
   with the five azimuths and the render size baked in. Nothing else can start
   until this is fixed.
2. **Greybox one unit end to end** — model, rig, render, quantise, validate,
   in-game with player colour and mirroring working. `docs/05` §6 step 2. One
   unit proves or disproves the whole route.
3. **Model the slice**: 12 units and 10 buildings, with the age costume and
   architecture variants as mesh swaps.
4. **Commission the icons and UI panel set** (§4.3) in parallel — they are off
   the critical path and do not depend on the render rig.

---

[genie]: https://en.wikipedia.org/wiki/Genie_Engine
[dosdays]: https://dosdays.co.uk/topics/Games/game_aoe.php
[pixellab]: https://www.pixellab.ai/pixellab-api
[retrodiffusion]: https://retrodiffusion.ai/
[rdterms]: https://www.retrodiffusion.ai/terms
[ogacc0]: https://opengameart.org/content/cc0-isometric
[usco]: https://www.copyright.gov/ai/
[pcgamer]: https://www.pcgamer.com/software/ai/steam-updates-ai-disclosure-form-to-specify-that-its-focused-on-ai-generated-content-that-is-consumed-by-players-not-efficiency-tools-used-behind-the-scenes/
