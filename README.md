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
use scriptorium::{AnchorMode, GrowthRequest, Scriptorium, Params, Rect, Seed};

let mut engine = Scriptorium::new();

// Glyph outlines are the host's own: an SVG path per character, normalized to
// a unit square (x: 0..1 left-to-right, y: 0..1 top-to-bottom). A character
// with no outline simply falls back to its bounding-box edge.
engine.outlines.insert("□", "M0,0 L1,0 L1,1 L0,1 Z");

let out = engine.illuminate(&GrowthRequest {
    seed: "00000000000000000009a5b2b9c4de6c".to_string(),
    time: 5_000.0,   // in the host's own unit — see `stages`
    host: Rect::new(0.0, 0.0, 600.0, 400.0),
    page: Some(Rect::new(-40.0, -40.0, 680.0, 480.0)),
    obstacles: vec![Rect::new(40.0, 200.0, 300.0, 14.0)],
    seeds: vec![Seed {
        box_rect: Rect::new(300.0, 180.0, 10.0, 14.0),
        mark_rect: Some(Rect::new(300.0, 180.0, 10.0, 14.0)),
        mode: AnchorMode::Edge,
        ch: Some("□".to_string()),
        ink_box: Some(Rect::new(300.0, 181.0, 10.0, 11.0)),
        size: None,
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

## License

MIT OR Apache-2.0, at your option.
