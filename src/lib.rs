// SPDX-License-Identifier: MIT OR Apache-2.0
#![doc = include_str!("../README.md")]
// `!(x > 0.0)` is deliberate throughout: it rejects NaN as well as zero
// and negatives, where `x <= 0.0` silently lets a NaN through. Every
// measurement here arrives from a host, and a NaN width must fail closed.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

pub mod anchor;
pub mod contour;
pub mod geom;
pub mod grammar;
pub mod hull;
pub mod params;
pub mod path;
pub mod render;
pub mod rng;
pub mod text;
pub mod turtle;

#[cfg(feature = "wasm")]
mod wasm;

use serde::{Deserialize, Serialize};

pub use anchor::{rail_for, resolve_anchors, AnchorMode, Outlines, Seed};
pub use contour::outer_contours;
pub use geom::{edge_point, rect_near, Point, Rect};
pub use grammar::{size_boost, Derivations};
pub use hull::{block_hull, convex_hull, rail_along, Glyph};
pub use params::{default_growth_stages, growth_stage, GrowthStage, Params, Production};
pub use render::{leaf_path, path_d, segment_paths, LeafPath, SegmentPath};
pub use rng::{Fixed, Rand, Rng};
pub use text::{leading_space_len, mark_lead_length};
pub use turtle::{interpret_from, Anchor, Leaf, Rail, Segment};

/// One layout's worth of everything the engine needs to grow a page's
/// decoration.
///
/// Every rectangle is in the host's own coordinate space — the same space the
/// returned path data is drawn in, so nothing needs a further transform.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GrowthRequest {
    /// Seeds the grammar AND the per-anchor streams: the same seed gives the
    /// same reading to every viewer. A block hash, a txid, a document id.
    pub seed: String,
    /// Depth, bucketed into a stage by [`growth_stage`]. Stage 0 grows
    /// nothing at all.
    pub confirmations: f64,
    /// The positioning context's own box. Only its size is read; growth is
    /// aimed away from its centre.
    pub host: Rect,
    /// The page's box, where there is a page to ask. Given one, growth stops
    /// just inside its edge and RUNS ALONG it, exactly as it traces any other
    /// boundary — which is what a manuscript's border does. Omitted, growth is
    /// bounded the older way: the host's own box plus [`Params::overflow`].
    pub page: Option<Rect>,
    /// One rectangle per term of rendered text, UNPADDED — the halo
    /// ([`Params::obstacle_pad`]) is applied here, scaled with everything else.
    ///
    /// Per term, not per line, and that is the point. A line rectangle spans
    /// the full measure whether or not the text reaches the end of it, so a
    /// field made of them leaves a vine nowhere to be except outside the
    /// paragraph altogether. Term boxes leave the page as it actually reads:
    /// the channel between two lines, the ragged end of a short line, the
    /// gutter beside a margin citation.
    ///
    /// The marks themselves belong OUT of this field: a vine grows off its own
    /// mark and along its outline, so it is inside that box by construction,
    /// and an obstacle you start inside of is a trap rather than a boundary.
    pub obstacles: Vec<Rect>,
    /// The page's blocks, one entry per block, each holding that block's own
    /// rendered characters and where each one puts ink.
    ///
    /// A block is whatever the host lays out as one: a paragraph, a script
    /// listing, a stanza. The convex hull of its LETTERS is the silhouette it
    /// presents to the rest of the page, and a mark that names its block (see
    /// [`Seed::block`]) grows by riding that silhouette counter-clockwise
    /// rather than by tracing its own letterform — the difference between a
    /// border ruled around a paragraph and a flourish off an initial.
    ///
    /// Characters and not the boxes they sit in, because a box is not a shape
    /// a reader sees: it carries the line's leading above and below the letter
    /// and the advance's side bearings either side of it. See [`block_hull`].
    ///
    /// Grouped rather than inferred, because only the host knows what belongs
    /// together: two columns of a table are not one block however near their
    /// glyphs fall, and a footnote under a paragraph is not part of it.
    pub blocks: Vec<Vec<Glyph>>,
    /// The marks to grow from.
    pub seeds: Vec<Seed>,
    /// The host's current body-text size. Marks at or below it earn no extra
    /// generations; the ratio to [`Params::reference_font_size`] scales every
    /// distance so the decoration stays in proportion to the letters when a
    /// reader scales the type.
    pub base_size: f64,
    pub params: Params,
    /// The stage ladder. Empty falls back to [`default_growth_stages`].
    pub stages: Vec<GrowthStage>,
}

/// How near a block's silhouette a mark's own ink has to come to count as
/// being ON it, in px at the reference body size.
///
/// The comparison is between a hull and the very points it was built from, so
/// a character that reaches the edge lands exactly on it and any tolerance at
/// all would do. Half a pixel is chosen to be describable rather than to be
/// necessary: nearer than the page could show.
const ON_SILHOUETTE: f64 = 0.5;

/// One anchor's finished decoration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorGrowth {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub size: f64,
    /// The extra generations this mark's own size earned it.
    pub boost: u32,
    pub segments: Vec<SegmentPath>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrowthResponse {
    /// One entry per seed, in the order they were given — empty when the
    /// stage is bare.
    ///
    /// An entry with no segments is a mark that had nothing to grow from: it
    /// named a block whose silhouette it does not touch. Reported rather than
    /// dropped, so a host can tell a mark that grew nothing from one it never
    /// asked about, and so the answer stays index-for-index with the request.
    pub anchors: Vec<AnchorGrowth>,
    /// Which stage the depth count fell in.
    pub stage: usize,
    /// How far the host's body size has drifted from the size the geometry was
    /// tuned at. Reported so a host can size its own debug overlay in step.
    pub geometry_scale: f64,
    /// The padded obstacle field the walk actually steered around — for a host
    /// that wants to draw it.
    pub obstacles: Vec<Rect>,
    /// One silhouette per block sent, in the same order, wound
    /// counter-clockwise — the rails the marks in each block rode. Reported
    /// for the same reason as `obstacles`: a host that wants to draw what
    /// growth was answering to should not have to recompute it.
    pub hulls: Vec<Vec<Point>>,
    /// The box growth was not allowed to leave.
    pub bounds: Rect,
}

/// The engine, and the state that must outlive a single layout: the symbolic
/// derivations (which must NOT change across a reflow, a resize or an
/// orientation flip) and the sampled glyph outlines.
#[derive(Default)]
pub struct Scriptorium {
    pub outlines: Outlines,
    derivations: Derivations,
}

impl Scriptorium {
    pub fn new() -> Self {
        Scriptorium::default()
    }

    /// Drops every cached derivation. Required after any GRAMMAR knob changes
    /// — see [`Derivations::reset`].
    pub fn reset_derivations(&mut self) {
        self.derivations.reset();
    }

    /// The symbol string for one source at one stage — exposed for a host that
    /// wants the grammar without the geometry.
    pub fn generate_symbol(
        &mut self,
        seed: &str,
        stage: usize,
        boost: u32,
        params: &Params,
        stages: &[GrowthStage],
    ) -> String {
        self.derivations.generate_symbol(seed, stage, boost, params, stages)
    }

    /// Grows one layout's worth of decoration.
    ///
    /// The symbolic derivation is cached; everything geometric here is
    /// recomputed from the rectangles handed in, which is the whole point of
    /// the split: a resize, an orientation change, a font finishing its load —
    /// none of that changes what grew, only where it fits.
    pub fn illuminate(&mut self, req: &GrowthRequest) -> GrowthResponse {
        let params = &req.params;
        let fallback_stages;
        let stages = if req.stages.is_empty() {
            fallback_stages = default_growth_stages();
            &fallback_stages
        } else {
            &req.stages
        };
        let stage = growth_stage(req.confirmations, stages);

        let base_size = if req.base_size > 0.0 { req.base_size } else { params.reference_font_size };
        let geometry_scale = if params.reference_font_size > 0.0 {
            base_size / params.reference_font_size
        } else {
            1.0
        };

        // The page's own edge, where there is a page to ask: growth stops just
        // inside it — enough that a stroke drawn on the boundary isn't
        // half-clipped — and, because leaving the bounds gets the same tangent
        // treatment as meeting an obstacle, runs along it rather than dying
        // against it. Lacking a page, the older bound stands: the host's own
        // box plus a modest overflow, so an anchor sitting right at an edge
        // doesn't spend its whole generation budget being blocked at step one.
        let bounds = match req.page {
            Some(p) => {
                let inset = params.bounds_inset * geometry_scale;
                Rect::new(
                    p.x + inset,
                    p.y + inset,
                    (p.w - inset * 2.0).max(0.0),
                    (p.h - inset * 2.0).max(0.0),
                )
            }
            None => {
                let overflow = params.overflow * geometry_scale;
                Rect::new(-overflow, -overflow, req.host.w + overflow * 2.0, req.host.h + overflow * 2.0)
            }
        };

        let pad = params.obstacle_pad * geometry_scale;
        let obstacles: Vec<Rect> = req.obstacles.iter().map(|r| r.padded(pad)).collect();

        // Each block's silhouette, drawn clear of the halo its own text
        // carries so that a vine riding one is not blocked by the very
        // paragraph it is wrapping.
        //
        // Kept in both forms: the bare hull is the writing's own shape, and is
        // what settles WHICH marks are on the silhouette at all; the grown one
        // is the ring they ride. Growing is a Minkowski sum over a handful of
        // vertices, so the second costs almost nothing once the first is had.
        let hull_pad = pad + params.hull_margin * geometry_scale;
        let bare: Vec<Vec<Point>> = req
            .blocks
            .iter()
            .map(|glyphs| block_hull(glyphs, &mut self.outlines, 0.0))
            .collect();
        let hulls: Vec<Vec<Point>> = bare.iter().map(|h| hull::grown(h, hull_pad)).collect();

        if stage == 0 || req.seed.is_empty() || req.seeds.is_empty() {
            return GrowthResponse {
                anchors: Vec::new(),
                stage,
                geometry_scale,
                obstacles,
                hulls,
                bounds,
            };
        }

        let step = params.step * geometry_scale;
        // A rail is walked by accumulating real distance between samples, so
        // it has to be sampled finer than the step that walks it — otherwise a
        // single step would cross a whole side of a paragraph.
        let anchors =
            resolve_anchors(&req.seeds, &req.host, &mut self.outlines, &hulls, (step / 4.0).max(0.5));

        let grown = anchors
            .iter()
            .zip(req.seeds.iter())
            .map(|(a, seed)| {
                // A mark that names a block grows only if it is ON that
                // block's silhouette. One buried in the middle of a line
                // shapes nothing: the ring nearest it belongs to whichever
                // letters do reach the edge, and a vine springing from there
                // would be decorating its neighbours' outline while claiming
                // to grow from this mark. Nothing to run means nothing grows.
                let exposed = match seed.block.and_then(|i| bare.get(i)) {
                    None => true, // no block named: a flourish off the letter, as ever
                    Some(ring) => {
                        let sigla = Glyph {
                            ch: seed.ch.clone(),
                            box_rect: seed.ink_box.unwrap_or(seed.mark_rect.unwrap_or(seed.box_rect)),
                        };
                        hull::on_hull(ring, &sigla, &mut self.outlines, ON_SILHOUETTE * geometry_scale)
                    }
                };
                if !exposed {
                    return AnchorGrowth {
                        x: a.x,
                        y: a.y,
                        angle: a.angle,
                        size: a.size,
                        boost: 0,
                        segments: Vec::new(),
                    };
                }

                // Each anchor's own size earns it extra generations on top of
                // the depth-driven stage — the SAME continuous derivation,
                // just carried further for a large mark than for an ordinary
                // one, so a page's one illuminated initial can visibly outgrow
                // its inline glyphs without becoming a different grammar.
                let boost = size_boost(a.size, base_size, params);
                let symbol = self.derivations.generate_symbol(&req.seed, stage, boost, params, stages);
                let mut rng = Rng::from_hex(&format!(
                    "{}:{}:{}:{},{}",
                    req.seed,
                    stage,
                    boost,
                    render::fixed(a.x, 0),
                    render::fixed(a.y, 0)
                ));
                // How far this anchor's decoration may roam, leashed to the
                // mark's own size — generous enough for a real flourish, but
                // never so wide it reads as belonging to a different part of
                // the page than the mark that grew it.
                //
                // Unless it is decorating a block, in which case the block is
                // what it belongs to: a border that stopped short of its own
                // paragraph would read as a failure rather than as restraint,
                // so a mark riding a silhouette is leashed to that
                // silhouette's own reach and no further.
                let mut max_reach = (params.max_reach_floor * geometry_scale)
                    .max(if a.size > 0.0 { a.size } else { 16.0 } * params.max_reach_mul);
                if let Some(ring) = seed.block.and_then(|i| hulls.get(i)) {
                    let lap = ring
                        .iter()
                        .map(|p| (p.x - a.x).hypot(p.y - a.y))
                        .fold(0.0_f64, f64::max);
                    max_reach = max_reach.max(lap);
                }
                // Only the obstacles this anchor's leash could ever bring it
                // near: a page's field runs into the thousands of term boxes,
                // and the walk tests every candidate step against every rect it
                // is handed. The margin is a step's worth either side of the
                // leash, the furthest a rect can sit and still block a
                // reachable step.
                let near: Vec<Rect> = obstacles
                    .iter()
                    .copied()
                    .filter(|r| rect_near(r, a.x, a.y, max_reach + step * 2.0))
                    .collect();
                let segments = interpret_from(
                    &symbol,
                    a,
                    &near,
                    Some(&bounds),
                    &mut rng,
                    max_reach,
                    geometry_scale,
                    params,
                );
                AnchorGrowth {
                    x: a.x,
                    y: a.y,
                    angle: a.angle,
                    size: a.size,
                    boost,
                    segments: segment_paths(&segments),
                }
            })
            .collect();

        GrowthResponse { anchors: grown, stage, geometry_scale, obstacles, hulls, bounds }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_mark_page() -> GrowthRequest {
        GrowthRequest {
            seed: "00000000000000000009a5b2b9c4de6c9c1c9b3e9e9a5b2b9c4de6c9c1c9b3e".to_string(),
            confirmations: 5000.0,
            host: Rect::new(0.0, 0.0, 600.0, 400.0),
            page: Some(Rect::new(-40.0, -40.0, 680.0, 480.0)),
            obstacles: vec![Rect::new(40.0, 200.0, 300.0, 14.0)],
            blocks: Vec::new(),
            seeds: vec![Seed {
                box_rect: Rect::new(300.0, 180.0, 10.0, 14.0),
                mark_rect: Some(Rect::new(300.0, 180.0, 10.0, 14.0)),
                mode: AnchorMode::Edge,
                ..Default::default()
            }],
            base_size: 16.0,
            params: Params::default(),
            stages: default_growth_stages(),
        }
    }

    /// The points of one segment's path data, back out of the string.
    fn points_of(d: &str) -> Vec<(f64, f64)> {
        d.split(['M', 'L'])
            .filter(|t| !t.is_empty())
            .map(|chunk| {
                let mut it = chunk.trim().split(',');
                (
                    it.next().unwrap().parse::<f64>().unwrap(),
                    it.next().unwrap().parse::<f64>().unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn a_bare_stage_grows_nothing_at_all() {
        let mut e = Scriptorium::new();
        let mut req = one_mark_page();
        req.confirmations = 0.0;
        let out = e.illuminate(&req);
        assert_eq!(out.stage, 0);
        assert!(out.anchors.is_empty());
    }

    #[test]
    fn a_confirmed_mark_grows_drawable_path_data() {
        let mut e = Scriptorium::new();
        let out = e.illuminate(&one_mark_page());
        assert_eq!(out.anchors.len(), 1);
        let a = &out.anchors[0];
        assert!(!a.segments.is_empty(), "a mark at a deep stage should grow something");
        assert!(a.segments.iter().any(|s| s.d.starts_with('M')), "segments carry SVG path data");
    }

    #[test]
    fn the_same_request_illuminates_identically_every_time() {
        let req = one_mark_page();
        let a = Scriptorium::new().illuminate(&req);
        let b = Scriptorium::new().illuminate(&req);
        assert_eq!(a, b, "two readers looking at the same source must see the same illumination");
        // …and a warm cache must not answer differently from a cold one.
        let mut warm = Scriptorium::new();
        warm.illuminate(&req);
        assert_eq!(warm.illuminate(&req), a);
    }

    #[test]
    fn a_reflow_changes_where_growth_lands_but_not_what_grew() {
        let mut e = Scriptorium::new();
        let req = one_mark_page();
        let symbol_at = |e: &mut Scriptorium, boost| {
            e.generate_symbol(&req.seed, 4, boost, &req.params, &req.stages)
        };
        let before = symbol_at(&mut e, 0);
        let mut narrow = req.clone();
        narrow.host = Rect::new(0.0, 0.0, 320.0, 900.0);
        e.illuminate(&narrow);
        assert_eq!(symbol_at(&mut e, 0), before, "a resize must not re-roll the derivation");
    }

    #[test]
    fn growth_stays_inside_the_page_it_was_given() {
        let mut e = Scriptorium::new();
        let req = one_mark_page();
        let out = e.illuminate(&req);
        let b = out.bounds;
        for a in &out.anchors {
            for s in &a.segments {
                for chunk in s.d.split(['M', 'L']).filter(|t| !t.is_empty()) {
                    let mut it = chunk.trim().split(',');
                    let (x, y) = (
                        it.next().unwrap().parse::<f64>().unwrap(),
                        it.next().unwrap().parse::<f64>().unwrap(),
                    );
                    // One decimal of print rounding, either side.
                    assert!(x >= b.x - 0.1 && x <= b.right() + 0.1, "x {x} left the page");
                    assert!(y >= b.y - 0.1 && y <= b.bottom() + 0.1, "y {y} left the page");
                }
            }
        }
    }

    #[test]
    fn obstacles_come_back_padded_by_the_halo_the_walk_actually_used() {
        let mut e = Scriptorium::new();
        let mut req = one_mark_page();
        req.params.obstacle_pad = 5.0;
        let out = e.illuminate(&req);
        assert_eq!(out.obstacles[0], Rect::new(35.0, 195.0, 310.0, 24.0));
    }

    #[test]
    fn the_halo_and_the_bounds_scale_with_the_readers_own_type_size() {
        let mut e = Scriptorium::new();
        let mut req = one_mark_page();
        req.base_size = 32.0; // a reader who has doubled the type
        let out = e.illuminate(&req);
        assert_eq!(out.geometry_scale, 2.0);
        assert_eq!(out.obstacles[0], Rect::new(35.0, 195.0, 310.0, 24.0));
    }

    #[test]
    fn with_no_page_the_host_box_plus_an_overflow_bounds_growth() {
        let mut e = Scriptorium::new();
        let mut req = one_mark_page();
        req.page = None;
        let out = e.illuminate(&req);
        assert_eq!(out.bounds, Rect::new(-48.0, -48.0, 696.0, 496.0));
    }

    #[test]
    fn a_larger_mark_earns_more_generations_than_an_ordinary_one() {
        let mut e = Scriptorium::new();
        let mut req = one_mark_page();
        req.seeds.push(Seed {
            box_rect: Rect::new(60.0, 60.0, 40.0, 54.0),
            mark_rect: Some(Rect::new(60.0, 60.0, 40.0, 54.0)),
            mode: AnchorMode::Edge,
            ..Default::default()
        });
        let out = e.illuminate(&req);
        assert_eq!(out.anchors[0].boost, 0, "a body-size mark is exactly ordinary");
        assert!(out.anchors[1].boost > 0, "a drop-cap-sized mark should outgrow it");
    }

    /// Three lines of a script listing, each opening with its opcode mark, and
    /// the mark on the middle line seeded to run the block's border.
    fn one_listing_page() -> GrowthRequest {
        let lines: Vec<Rect> =
            (0..3).map(|i| Rect::new(100.0, 100.0 + f64::from(i) * 20.0, 260.0, 14.0)).collect();
        GrowthRequest {
            // No outlines registered, so each line's ink is its own box —
            // which keeps this fixture about the border and not about a font.
            blocks: vec![lines.iter().map(|&box_rect| Glyph { ch: None, box_rect }).collect()],
            obstacles: lines,
            seeds: vec![Seed {
                box_rect: Rect::new(100.0, 120.0, 10.0, 14.0),
                mark_rect: Some(Rect::new(100.0, 120.0, 10.0, 14.0)),
                mode: AnchorMode::Edge,
                block: Some(0),
                ..Default::default()
            }],
            ..one_mark_page()
        }
    }

    #[test]
    fn a_mark_that_names_its_block_runs_that_blocks_border() {
        let mut e = Scriptorium::new();
        let req = one_listing_page();
        let out = e.illuminate(&req);

        let hull = &out.hulls[0];
        assert!(hull.len() >= 4, "a three-line block has a silhouette");

        let on_hull = |x: f64, y: f64| {
            hull.iter().enumerate().any(|(i, a)| {
                let b = hull[(i + 1) % hull.len()];
                // Distance from the point to that hull edge, the plain way.
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let len2 = dx * dx + dy * dy;
                let t = if len2 > 0.0 {
                    (((x - a.x) * dx + (y - a.y) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (x - (a.x + t * dx)).hypot(y - (a.y + t * dy)) < 1.5
            })
        };
        // The opening run is the ride. It is not `segments[0]`: a segment is
        // pushed where it CLOSES, so the deepest branch of the derivation
        // lands first and the run it forked off lands last. It is the longest
        // one that starts where the walk did.
        let a = &out.anchors[0];
        let opening = a
            .segments
            .iter()
            .map(|s| points_of(&s.d))
            .filter(|p| {
                p.first()
                    .is_some_and(|(x, y)| (x - a.x).abs() < 0.1 && (y - a.y).abs() < 0.1)
            })
            .max_by_key(Vec::len)
            .expect("the walk starts somewhere");

        assert!(opening.len() > 4, "the ride is more than a step or two");
        let ridden = opening.iter().filter(|(x, y)| on_hull(*x, *y)).count();
        assert!(ridden >= opening.len() - 2, "{ridden} of {} points on the hull", opening.len());
    }

    #[test]
    fn a_mark_riding_a_border_is_leashed_to_that_border_and_no_further() {
        let mut e = Scriptorium::new();
        let mut req = one_listing_page();
        // A leash far too short to reach round a block on its own.
        req.params.max_reach_floor = 10.0;
        req.params.max_reach_mul = 0.0;
        req.params.glyph_follow_max = 400;
        let out = e.illuminate(&req);

        let a = &out.anchors[0];
        let far = out.hulls[0]
            .iter()
            .map(|p| (p.x - a.x).hypot(p.y - a.y))
            .fold(0.0_f64, f64::max);
        let reached = a
            .segments
            .iter()
            .flat_map(|s| points_of(&s.d))
            .map(|(x, y)| (x - a.x).hypot(y - a.y))
            .fold(0.0_f64, f64::max);
        assert!(reached > 50.0, "the border is run, not cut off at the mark's own leash");
        assert!(reached <= far + 1.0, "and not one step further than the block it decorates");
    }

    #[test]
    fn a_mark_the_silhouette_does_not_touch_grows_nothing_at_all() {
        let mut e = Scriptorium::new();
        let mut req = one_listing_page();
        // The same mark, moved from the head of its line into the middle of
        // it: it shapes nothing, so there is no border of its own to run.
        req.seeds[0].box_rect = Rect::new(220.0, 120.0, 10.0, 14.0);
        req.seeds[0].mark_rect = Some(Rect::new(220.0, 120.0, 10.0, 14.0));
        let out = e.illuminate(&req);
        assert_eq!(out.anchors.len(), 1, "the mark is still reported, in the order it was given");
        assert!(out.anchors[0].segments.is_empty(), "but nothing grew from it");

        // …and it is the SILHOUETTE that decides, not the position: the same
        // buried mark naming no block grows off its own letterform as ever.
        req.seeds[0].block = None;
        assert!(!e.illuminate(&req).anchors[0].segments.is_empty());
    }

    #[test]
    fn a_border_runs_counter_clockwise_from_the_mark_it_grew_from() {
        let mut e = Scriptorium::new();
        let out = e.illuminate(&one_listing_page());
        let a = &out.anchors[0];
        // The mark opens the middle line, against the block's left edge.
        // Counter-clockwise, as the page is read, that heads DOWN the margin.
        assert!(a.angle.sin() > 0.5, "the ride sets off toward the foot of the block");
        assert!(a.angle.cos().abs() < 0.3, "along the edge, not across the text");
    }

    #[test]
    fn a_block_a_mark_does_not_name_leaves_it_growing_off_its_own_letter() {
        let mut e = Scriptorium::new();
        let mut req = one_listing_page();
        let bordered = e.illuminate(&req);
        req.seeds[0].block = None;
        let free = e.illuminate(&req);
        assert_ne!(
            bordered.anchors[0].segments[0].d, free.anchors[0].segments[0].d,
            "naming a block has to change where growth goes, or it means nothing"
        );
        assert_eq!(
            bordered.hulls, free.hulls,
            "the silhouettes are the page's, not any one mark's"
        );
    }

    #[test]
    fn the_silhouette_clears_the_halo_of_the_text_it_wraps() {
        let mut e = Scriptorium::new();
        let mut req = one_listing_page();
        req.params.obstacle_pad = 3.0;
        req.params.hull_margin = 5.0;
        let out = e.illuminate(&req);
        // Every padded obstacle is strictly inside the ring the vine rides,
        // which is what stops the ride being blocked on its first step.
        for o in &out.obstacles {
            for p in &out.hulls[0] {
                assert!(!o.contains(p.x, p.y), "the border {p:?} runs through the text {o:?}");
            }
        }
    }

    #[test]
    fn a_seedless_or_unseeded_request_grows_nothing_rather_than_guessing() {
        let mut e = Scriptorium::new();
        let mut req = one_mark_page();
        req.seeds.clear();
        assert!(e.illuminate(&req).anchors.is_empty());
        let mut req = one_mark_page();
        req.seed = String::new();
        assert!(e.illuminate(&req).anchors.is_empty());
    }
}
