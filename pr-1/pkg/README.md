# scriptorium

A procedural manuscript-illumination engine.

Vines grown from the marks a page already carries, the way a scribe's
marginalia grows from a manuscript's initials: a stochastic L-system whose
derivation is a pure function of a seed and a depth, walked by a turtle that
rides the seed glyph's own silhouette, traces the text around it, follows the
page's edge, and stays leashed to the mark it grew from.

The engine is host-agnostic. It measures nothing and draws nothing: a host
hands it rectangles it has already laid out and gets SVG path data back. That
is what lets the whole thing be tested with `cargo test` on a bare checkout —
no browser, no font, no canvas — and what lets the same crate serve a web page
(via `--features wasm`), a PDF renderer, or a plotter.

## The split that matters

Two things are computed on very different schedules, and keeping them apart is
the design:

- The **symbolic derivation** — which branches exist, how the grammar unfolded
  — is a pure function of (seed, stage) and is cached forever. A viewport never
  re-rolls it.
- The **turtle interpretation** — where those branches land in pixels, which
  ones got redirected or cut short by a line of text — is recomputed on every
  layout pass, cheaply, from the host's own measured rectangles.

A resize, an orientation change, a font finishing its load: none of that
changes what grew, only where it fits.

## What growth answers to

In the order it binds:

- **The glyph**, which a vine leaves by riding its own outer contour, forking
  away where following it further would close the letter back onto the trail
  just drawn. Counters are holes, not edges, and are never ridden.
- **The block**, for a mark that names one: the convex hull of a paragraph's
  own LETTERS — their outlines, not the boxes they sit in — is the silhouette
  it presents to the page, and growth rides it counter-clockwise, beginning at
  the mark's own clockwise-most extent so that the first thing it does is wrap
  the stretch of ring that mark is exposed along. Start anywhere else and the
  ride sets off from part-way across the letter it grew from, leaving the rest
  of that letter behind it for good. This is the border a scribe rules around a
  paragraph rather than the flourish that leaves an initial. The ragged end of
  a short last line is inside that silhouette, not traced by it, and the ring
  is ruled clear of the writing so that a vine riding it is not blocked by the
  paragraph it wraps.
- **The text**, as one padded rectangle per term the host laid out. Growth
  traces those boxes' silhouettes the way a scribe's vine runs between the
  lines — per term, not per line, so the channel between two lines and the
  ragged end of a short one stay open ground.
- **The page**, whose own edge bounds growth and, being an edge like any other,
  gets ridden rather than crashed into.
- **The leash**, so what grows off a mark still reads as belonging to it and
  not to some other part of the page.

## Using it

```rust
use scriptorium::{AnchorMode, Glyph, GrowthRequest, Scriptorium, Params, Rect, Seed};

let mut engine = Scriptorium::new();

// Glyph outlines are the host's own: an SVG path per character, normalized to
// a unit square (x: 0..1 left-to-right, y: 0..1 top-to-bottom). A character
// with no outline simply falls back to its bounding-box edge.
engine.outlines.insert("□", "M0,0 L1,0 L1,1 L0,1 Z");

let out = engine.illuminate(&GrowthRequest {
    seed: "00000000000000000009a5b2b9c4de6c".to_string(),
    confirmations: 5_000.0,
    host: Rect::new(0.0, 0.0, 600.0, 400.0),
    page: Some(Rect::new(-40.0, -40.0, 680.0, 480.0)),
    obstacles: vec![Rect::new(40.0, 200.0, 300.0, 14.0)],
    // The page's blocks, each one the characters the host laid out in it and
    // where each puts ink. A mark that names its block grows by riding that
    // block's silhouette, counter-clockwise, instead of by tracing its own
    // letterform. A character with no outline registered wraps by its box.
    blocks: vec![vec![
        Glyph::new("□", Rect::new(40.0, 201.0, 10.0, 11.0)),
        Glyph::new("□", Rect::new(52.0, 201.0, 10.0, 11.0)),
    ]],
    seeds: vec![Seed {
        box_rect: Rect::new(300.0, 180.0, 10.0, 14.0),
        mark_rect: Some(Rect::new(300.0, 180.0, 10.0, 14.0)),
        mode: AnchorMode::Edge,
        ch: Some("□".to_string()),
        ink_box: Some(Rect::new(300.0, 181.0, 10.0, 11.0)),
        size: None,
        block: None, // `Some(0)` to run the border instead
    }],
    base_size: 16.0,
    params: Params::default(),
    stages: Vec::new(), // the default ladder
});

for anchor in &out.anchors {
    for segment in &anchor.segments {
        // segment.d is ready for an SVG <path>; segment.leaves carry their own
        // blade/vein path data and the transform that places them.
        assert!(segment.d.is_empty() || segment.d.starts_with('M'));
    }
}
```

Change a **grammar** knob (`params.productions`, the branchiness ramp,
`max_size_boost`) and call [`Scriptorium::reset_derivations`] — otherwise a
seed keeps answering with generations grown under the old rules. Change a
**turtle** knob and a plain re-layout is enough; nothing geometric is cached.

## Building for the web

```sh
wasm-pack build --target web --no-default-features --features wasm
```

The `wasm` feature exposes the same API over `wasm-bindgen`, taking and
returning plain JSON-shaped objects with camelCase keys, so a host page's
measuring layer can hand over rectangles and get path data back without
mirroring any of the logic in JavaScript.

## The playground

`web/` is a reference host: a page of prose with an illuminated initial, and
every knob in [`Params`] wired to it.

```sh
web/build.sh --serve      # builds web/pkg, then serves http://localhost:8080
```

You need not build it to look at it: every pull request publishes its own copy
to `https://asherp.github.io/scriptorium/pr-<number>/`, and `main` publishes to
the root, so a change to how growth behaves can be reviewed by looking at it.

It is worth having as more than a demo, because it makes the split above
visible. The panel labels each knob **grammar** or **turtle** and clears the
derivation cache only for the first kind; drag the column narrower or scale the
type and the readout's symbol string does not move, while the vine relaxes into
its new measure. The debug overlays draw the padded obstacle field, the bounds,
the block silhouettes, and the contours the vines are riding.

Its notation is Bitcoin Script, one mark per opcode — `⧉` for DUP, `⌖` for
HASH160, `∇` for CHECKSIG, all 110 of them. Every mark is a seed, the list is
a reader's to edit an opcode or a whole group at a time, and any word can be
made a mark or unmade with a click. That the engine has no idea any of this is
happening is the point: it is handed rectangles, and what those rectangles mean
stays in the host.

Everything DOM-shaped lives there too, as the contract requires. `web/` lays
the page out one paragraph per block and one span per term, measures those
boxes back, works out where each rendered CHARACTER puts ink — neither box the
DOM will hand over is the ink, so the metrics come from a canvas and the range
rectangle only anchors them — and, since a browser will not hand over a font's
contours, gets its glyph outlines by rasterizing each character and tracing the
boundary between ink and paper. The engine is handed rectangles and
unit-square paths, exactly as a PDF renderer or a plotter would hand it the
same.

Two things are worth knowing when growing borders rather than flourishes.

How far a vine runs its border is a **grammar** question, not a turtle one. A
branch springs off the rail rather than along it, so the ride is exactly as
long as the run of `F`s outside any `[` — the readout reports that count, and
it predicts the ride to the step. Everything else in the symbol is growth that
leaves the ring. **follow steps** is only a cap on top of that and does not
bind until the trunk outgrows it; to run more of a block, give
[`Params::productions`] more trunk rather than raising the cap.

**border margin** is how far outside the text the silhouette is ruled, and it
has to clear a line's leading, not just its ink, wherever the obstacle field is
per term.

## License

MIT OR Apache-2.0, at your option.
