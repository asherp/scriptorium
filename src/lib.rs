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
    /// How long this thing has stood, in whatever unit the host counts in —
    /// confirmations, days, revisions, seconds. Bucketed into a stage by
    /// [`growth_stage`] against [`GrowthRequest::stages`], which is where the
    /// unit is actually decided: the engine only ever compares this against
    /// that ladder's own bounds, and never attaches a meaning to it. Stage 0
    /// grows nothing at all.
    ///
    /// Deliberately not a duration type. A host that counts in blocks or in
    /// edits has no duration to give, and the ladder is a set of thresholds in
    /// the host's own units rather than in seconds.
    pub time: f64,
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
    pub anchors: Vec<AnchorGrowth>,
    /// Which stage [`GrowthRequest::time`] fell in.
    pub stage: usize,
    /// How far the host's body size has drifted from the size the geometry was
    /// tuned at. Reported so a host can size its own debug overlay in step.
    pub geometry_scale: f64,
    /// The padded obstacle field the walk actually steered around — for a host
    /// that wants to draw it.
    pub obstacles: Vec<Rect>,
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
        let stage = growth_stage(req.time, stages);

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

        if stage == 0 || req.seed.is_empty() || req.seeds.is_empty() {
            return GrowthResponse {
                anchors: Vec::new(),
                stage,
                geometry_scale,
                obstacles,
                bounds,
            };
        }

        let anchors = resolve_anchors(&req.seeds, &req.host, &mut self.outlines);
        let step = params.step * geometry_scale;

        let grown = anchors
            .iter()
            .map(|a| {
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
                let max_reach = (params.max_reach_floor * geometry_scale)
                    .max(if a.size > 0.0 { a.size } else { 16.0 } * params.max_reach_mul);
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

        GrowthResponse { anchors: grown, stage, geometry_scale, obstacles, bounds }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_mark_page() -> GrowthRequest {
        GrowthRequest {
            seed: "00000000000000000009a5b2b9c4de6c9c1c9b3e9e9a5b2b9c4de6c9c1c9b3e".to_string(),
            time: 5000.0,
            host: Rect::new(0.0, 0.0, 600.0, 400.0),
            page: Some(Rect::new(-40.0, -40.0, 680.0, 480.0)),
            obstacles: vec![Rect::new(40.0, 200.0, 300.0, 14.0)],
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

    #[test]
    fn a_bare_stage_grows_nothing_at_all() {
        let mut e = Scriptorium::new();
        let mut req = one_mark_page();
        req.time = 0.0;
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
