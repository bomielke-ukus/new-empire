# The new look's UI

Phase 6 of the revamp. The world became a sunlit painted diorama; the UI
was still a chunky pixel font on flat brown boxes. This is the skin that
puts the HUD and the screens in the same world: parchment and limestone,
bronze and gold, deep ink, a lapis accent and terracotta for trouble. It is
a skin, not a redesign: the layout, every hit box, the text, the keys and
the accessibility features (UI scale, the players' symbols, the controls
overlay) are the old UI's. It is drawn with the new art on
(`Settings::new_art`, `mapview --look 1`); with it off the UI is the old
one to the pixel, which the old golden images hold.

Code: `crates/view/src/skin.rs` (palette, font, boxes, icons), the
`Painter` in `crates/view/src/hud.rs` (how the HUD and screens draw), and
`tools/look/ui_font.py` (the baked type).

## Palette

From the buildings' pigments (`tools/look/lib.py`, `PAL`), as sRGB.

| Name | Hex | From | Use |
| --- | --- | --- | --- |
| Parchment | `#F1E5C7` | plaster, warmed | panels, plates |
| Stone | `#E4CFA0` | limestone | a button at rest |
| Stone, lit | `#F7E8BE` | limestone in sun | a hovered button |
| Stone, drained | `#CCC4B4` | | a greyed button |
| Ink | `#2C1C12` | wood_dark to black | text |
| Rubric | `#802A14` | tile_dark, deepened | headings, the old gold text |
| Cream | `#FAF3E2` | linen | text on a dark chip or key cap |
| Muted | `#544A3E` | | text on a greyed button |
| Bronze, dark | `#603C1A` | bronze in shadow | every frame's outer edge |
| Bronze | `#CC8F3D` | bronze | a panel's frame, rules |
| Gold line | `#DEAC3E` | gold | the thin line inside a panel, the hover edge |
| Lapis | `#1F389E` | lapis | the accent (a cheat's notice) |
| Terracotta | `#A63C20` | terracotta, deepened | warnings |
| Terracotta wash | `#ECB296` | | a flashed resource |
| Ochre | `#ECBE5C` | yellow_ochre | gold chips: idle villagers, the F1 hint, a newly opened button's halo |
| Malachite | `#2E9654` | malachite | health |
| Red ochre | `#C43A26` | red_ochre | low health |
| Gold | `#F0B430` | gold | progress |
| Trough | `#422C1A` | wood_dark | bars' troughs, the minimap's well, key caps |

Contrast (WCAG 2, every pair the skin draws text in; the test
`every_text_pair_meets_wcag_aa` holds them at 4.5:1 or better): ink on
parchment 13.1, rubric on parchment 7.4, ink on stone 10.7, ink on lit
stone 13.4, muted on drained stone 5.0, rubric on drained stone 5.4, cream
on terracotta 5.8, ink on the terracotta wash 8.9, ink on ochre 9.4, rubric
on stone 6.1, cream on lapis 9.0, ink on gold 8.8, cream on the trough
11.8.

## Type

Two faces under the SIL Open Font License 1.1 (`assets/ui/OFL.txt`):
**Carlito Bold**, a sturdy humanist sans, for everything at the HUD's text
size, and **Caladea Bold**, a serif with some classical weight, for text at
twice that and up: the selection's name, the resource counts, headings, the
age banner and the title. All the game's text is capitals, set with a
little tracking (the serif more, for an inscription's spacing), kerned.

The faces are baked, not shipped: `tools/look/ui_font.py` renders each
with FreeType, hinted, at every cap height the UI asks for at the UI
scales the game offers (sans 7, 9, 11, 14, 18, 21, 28 px; serif 14 to 84
px, the large sizes with only the title's characters) into one grey
coverage atlas, `assets/ui/font.png`, with the metrics in
`assets/ui/font.txt` (advances, bearings, kerning pairs). When the atlas is
built the glyphs are coloured in the skin's four inks. Text is always
drawn at a bake's own pixel size, never scaled, picked by the cap height
the UI wants in device pixels (`7 × text scale × UI scale`), and every
glyph lands on a whole device pixel: crisp at 1×, 1.25×, 1.5×, 2× and up.
Advances are the hinter's whole pixels and the tracking and kerning are
rounded, so the spacing is even.

Layout keeps measuring as it always did: a panel's size, a button's
width, a briefing's line count come from the old font's metrics
(`font::width`, `font::wrap`), so nothing that is clicked moves. What is
drawn is measured in the skin's type (`Painter::measure`, `Painter::fit`):
centring, right-aligning and cutting to fit. The skin's sans is narrower
than the old font for the HUD's words, so what fitted still fits.

## Boxes

Every box is nine pieces (four corners, four edges, a fill), rendered when
the atlas is built for each of seven device scales (1, 1.25, 1.5, 2, 2.5,
3, 4) so that their rounded corners and lines fall on whole device pixels;
a UI scale between them uses the nearest. Over a box's face lies a sheen
(light at the top, shade at the foot) for a soft bevel; a panel's
parchment has a faint fibre grain, tiled a device pixel to a pixel; a box
standing over the world casts a soft drop shadow.

- **Panel**: parchment; a dark bronze edge, bronze, parchment, then a thin
  gold line; corners rounded 6 px. The resource bar and the bottom panel
  are panels whose ends run off the window.
- **Plate**: parchment with a light inner edge, rounded 4 px: tooltips,
  notices, the hint, objectives, the narrator, the Wonder clocks, the
  minimap's frame, a line over the world (the banner's subtitle).
- **Buttons**: rest is limestone with a lit inner edge and a dark bronze
  edge; hover is lit limestone with a gold edge; greyed is drained stone
  with a pewter edge and muted text. A button newly opened by a technology
  has an ochre halo round it. (The UI has no pressed state of its own; a
  click acts at once.)
- **Chips**: terracotta with cream text for a warning (a full house, a
  farm without wood, a refused reason in a tooltip, an error on a screen);
  the terracotta wash for a flashed resource; ochre for the idle count and
  the F1 hint.
- **Trough**: a sunken dark bronze-brown, for bars, key caps, the minimap's
  well and the setup screen's preview.

## The HUD

- **Resource bar**: a parchment band. Each resource's own new art (the
  berry bush, a tree, the stone and gold mines, area-filtered to icon
  size) stands for its name beside its count in the serif, the worker
  count small after it; then population, idle villagers (an ochre chip),
  the civilization and age, warnings (terracotta chips) and the F1 hint.
  It reflows as before when the window is narrow. Without the icons
  (no new art), the names are written.
- **Selection panel**: the name in the serif, the facts in the sans, the
  health as a bar in a trough (malachite, red ochre under half). The
  production queue's boxes are stone buttons with each unit's own picture,
  gold rising behind the head as it trains; a technology keeps its
  letter.
- **Command grid**: stone buttons. A build or train button at least 80
  HUD px wide shows the building's or unit's own idle frame, in the
  player's colour, at the left; the label and cost beside it; the key on a
  small dark key cap at the bottom right, the key the player bound it to.
  Narrower buttons keep the label alone. Stop, stances, formations and
  technologies keep their words.
- **Tooltip**: a plate with a shadow; the first line in rubric; a greyed
  button's reason on a terracotta chip.
- **Notifications**: plates, each marked with a small chip of its kind:
  terracotta an attack, pewter a loss, ochre research and ages, lapis a
  cheat.
- **The minimap** sits in a trough set in a plate, a framed medallion at
  the panel's corner. (The diamond pokes above the panel as it always did;
  the plate frames it.)
- **Health bars over the world**: small troughs with the bar inside.
- **Banner**: a panel with the serif title in rubric; its subtitle stands
  on a plate so it reads over the world.
- **Overlays** (controls, performance, objectives, the narrator, the cheat
  line): panels and plates.

## The screens

The title, setup, load, settings (both pages), campaigns, briefing, pause,
results and the scenario editor's strips all use the same panel, buttons,
warnings and type; a screen with no world behind it stands on a dark warm
backdrop. The title is the serif at four times the text size. The setup
screen's map preview lies in a trough. One fix that is the skin's alone:
the setup screen's rows run one row past the height the panel allows for
them (`3 + opponents + 2` rows are counted, `4 + opponents + 2` drawn), so
the civilization's lines sat on the seed row; with the skin they are set
clear of it. Nothing that is clicked moves; the old UI keeps the overlap,
as its golden image does.

## How it reaches the screen

Everything the skin draws is a frame on the atlas's colour pages, added
when the atlas is built with the skin (`Atlas::with_sheets_skinned`; the
game passes `Settings::new_art`): the glyphs in each ink, the boxes'
pieces and shadows at each scale, its flat colours, the sheen and grain,
and the icons and portraits made from the colour sets already loaded. The
GPU renderer and the software rasteriser draw them as they draw any colour
frame (straight alpha, sampled texel for texel), so neither needed a
change. The painter places the skin's sprites in device pixels, stored in
HUD pixels so the UI's scale on the way out lands them back on the grid.
A box is about a dozen sprites, a line of text a sprite a glyph; the HUD
stays a few hundred sprites a frame. The skin adds a few megabytes to the
colour pages (most of it the type at the larger scales).

Pictures: the golden images `look-hud`, `look-title-screen` and
`look-setup-screen` under `tools/mapview/tests/golden-images`; and
`mapview --look 1 --hud 1 --select 6 --notices 1`, `--screen
title|setup|settings|load|campaigns|briefing|pause|results|editor`.
