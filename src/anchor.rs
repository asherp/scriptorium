// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Seed points vines grow from — the marks already on the page.
//
// The host measures; this decides. Everything here is arithmetic over
// rectangles the host has already laid out, plus the glyph outlines it
// registered, so nothing in this file needs a DOM, a font or a canvas.
//
// Each anchor's POSITION sits just past the mark's own boundary, not at its
// centroid — a vine reads as growing OFF the glyph's edge, not sprouting out
// of its middle — on whichever side faces away from the host's own center,
// which in practice aims a mark near a text column out toward the margin
// rather than back across the prose.
//
// The anchor's initial growth TANGENT is a different question, and answered
// separately: not which side of the glyph to start from, but which way to head
// once there. Given a real outline to trace, growth starts running along the
// letter's own silhouette, the way it will trace any OTHER obstacle's edge
// once under way. Lacking one, the mark's plain bounding-box edge stands in.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::contour::outer_contours;
use crate::geom::{angle_diff, edge_point, Point, Rect};
use crate::path::sample_subpaths;
use crate::turtle::{Anchor, Rail};

/// How finely a glyph outline is resampled. The sampling is the expensive part
/// and does not depend on the size the glyph happens to be drawn at, which is
/// pure arithmetic applied afterwards.
const OUTLINE_SAMPLES: usize = 220;

/// Where on a mark growth starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnchorMode {
    /// Just past the mark's own boundary, on the side facing away from the
    /// host's centre. The default, and the only mode that rides an outline.
    #[default]
    Edge,
    /// The mark's centroid.
    Center,
    /// The containing box's corner — for a mark with no box of its own, e.g. a
    /// CSS `::first-letter` drop cap.
    TopLeft,
}

/// One measured mark, as the host sees it. All rectangles are in the host's
/// own coordinate space.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Seed {
    /// The seed element's own box.
    #[serde(rename = "box")]
    pub box_rect: Rect,
    /// The box of just the mark's own characters, where the host could measure
    /// it. The difference is not small: a composite like `β₃₂` measures about
    /// 2.5x wider as a span than the `β` itself, and a container seed — a
    /// citation line, the paragraph a drop cap opens — is a whole block box
    /// whose corner has nothing to do with where the mark is.
    pub mark_rect: Option<Rect>,
    pub mode: AnchorMode,
    /// An explicit size override, for a mark with no box of its own. Falls
    /// back to the mark rectangle's height.
    pub size: Option<f64>,
    /// The mark's first character — what an outline is looked up by.
    pub ch: Option<String>,
    /// Where that character actually puts ink, which is where the unit-square
    /// outline maps onto. Not the same box as the element's own: an inline box
    /// carries the line's leading above and below the letter and spans every
    /// character in the mark.
    pub ink_box: Option<Rect>,
}

/// The glyph outlines a host has registered, plus the sampled contours derived
/// from them.
///
/// Deliberately supplied by the host rather than baked in: which characters a
/// vine can trace is a property of the host's own notation and its own font,
/// not of the engine. An outline is an SVG path in a UNIT SQUARE (`x`: 0..1
/// left-to-right, `y`: 0..1 top-to-bottom, the screen's own convention rather
/// than a font's Y-up one), so a caller holding the character's measured ink
/// box maps straight into it with no separate font-size factor.
#[derive(Default)]
pub struct Outlines {
    paths: HashMap<String, String>,
    contours: HashMap<String, Vec<Vec<Point>>>,
}

impl Outlines {
    pub fn new() -> Self {
        Outlines::default()
    }

    /// Replaces the whole table. Any cached sampling is dropped with it.
    pub fn set(&mut self, paths: HashMap<String, String>) {
        self.paths = paths;
        self.contours.clear();
    }

    pub fn insert(&mut self, ch: impl Into<String>, d: impl Into<String>) {
        let ch = ch.into();
        self.contours.remove(&ch);
        self.paths.insert(ch, d.into());
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn contains(&self, ch: &str) -> bool {
        self.paths.contains_key(ch)
    }

    /// A glyph's outline sampled into plain polylines in the unit square it
    /// was stored in — one per subpath, reduced to the outer ones, cached per
    /// character.
    fn unit_contours(&mut self, ch: &str) -> Option<&Vec<Vec<Point>>> {
        if !self.contours.contains_key(ch) {
            let d = self.paths.get(ch)?;
            let sampled = outer_contours(sample_subpaths(d, OUTLINE_SAMPLES));
            self.contours.insert(ch.to_string(), sampled);
        }
        self.contours.get(ch).filter(|c| !c.is_empty())
    }
}

/// The rail a vine rides out of its mark, or `None` where there is nothing to
/// ride.
///
/// Only the letter's OUTER contours are candidates — a counter is a hole, and
/// a vine that rode one would be crawling around inside the glyph — and of
/// those the nearest to the start point wins, which for a mark drawn as
/// several separate strokes is the stroke the vine actually grows off.
///
/// Returns `None` when the character has no outline (not every mark in a
/// notation does), when its ink can't be measured, or when the nearest contour
/// is too short to have a direction at all — for the caller to fall back on
/// the mark's plain bounding-box edge.
pub fn rail_for(
    outlines: &mut Outlines,
    ch: &str,
    ink_box: &Rect,
    host_x: f64,
    host_y: f64,
    outward: f64,
) -> Option<Rail> {
    if !(ink_box.w > 0.0) || !(ink_box.h > 0.0) {
        return None;
    }
    let contours = outlines.unit_contours(ch)?;

    let mut best: Option<(Vec<Point>, usize, f64)> = None;
    for unit in contours {
        let pts: Vec<Point> = unit
            .iter()
            .map(|p| Point::new(ink_box.x + p.x * ink_box.w, ink_box.y + p.y * ink_box.h))
            .collect();
        let mut bi = 0;
        let mut bd = f64::INFINITY;
        for (i, p) in pts.iter().enumerate() {
            let dist = (p.x - host_x).powi(2) + (p.y - host_y).powi(2);
            if dist < bd {
                bd = dist;
                bi = i;
            }
        }
        let better = match &best {
            None => true,
            Some((_, _, d)) => bd < *d,
        };
        if better {
            best = Some((pts, bi, bd));
        }
    }

    let (pts, start_idx, _) = best?;
    let n = pts.len();
    if n < 3 {
        return None;
    }
    // Which way round the contour continues most nearly outward — the same
    // question, and the same answer, the plain bounding-box fallback settles
    // with `tangent_angles`.
    let at = |i: usize| pts[i % n];
    let fwd = (at(start_idx + 2).y - at(start_idx).y).atan2(at(start_idx + 2).x - at(start_idx).x);
    let dir = if angle_diff(fwd, outward) <= angle_diff(fwd + std::f64::consts::PI, outward) { 1 } else { -1 };
    let tangent = if dir == 1 { fwd } else { fwd + std::f64::consts::PI };
    let (mut cx, mut cy) = (0.0, 0.0);
    for p in &pts {
        cx += p.x;
        cy += p.y;
    }
    Some(Rail { pts, start_idx, dir, tangent, cx: cx / n as f64, cy: cy / n as f64 })
}

/// Turns the host's measured marks into anchors ready to grow from.
pub fn resolve_anchors(seeds: &[Seed], host: &Rect, outlines: &mut Outlines) -> Vec<Anchor> {
    let cx = host.w / 2.0;
    let cy = host.h / 2.0;
    seeds
        .iter()
        .map(|seed| {
            // The mark's own characters, not the element's box — so a count
            // riding a mark neither shifts the point growth starts from nor
            // inflates the size that earns it generations. The two overridden
            // modes are about the ELEMENT's box by construction.
            let r = match seed.mode {
                AnchorMode::Edge => seed.mark_rect.unwrap_or(seed.box_rect),
                _ => seed.box_rect,
            };
            let c = r.center();
            let angle = {
                let a = (c.y - cy).atan2(c.x - cx);
                if a.is_finite() && a != 0.0 {
                    a
                } else {
                    0.0
                }
            };
            // Height, not the larger of width/height: an inline mark's
            // rendered SIZE is its glyph height; its width is mostly a
            // function of how many characters happen to be in the mark, which
            // says nothing about how big it looks.
            let size = match seed.size {
                Some(s) if s != 0.0 && s.is_finite() => s,
                _ => r.h,
            };

            let (x, y) = match seed.mode {
                // Right at the corner, not pushed out past it: the vine has to
                // read as growing FROM the initial, touching it, the way real
                // marginalia is physically continuous with the letter it
                // decorates.
                AnchorMode::TopLeft => (r.x, r.y),
                AnchorMode::Center => (c.x, c.y),
                AnchorMode::Edge => {
                    let p = edge_point(&r, angle);
                    (p.x + angle.cos() * 1.5, p.y + angle.sin() * 1.5)
                }
            };

            let mut growth_angle = angle;
            let mut rail = None;
            if seed.mode == AnchorMode::Edge {
                let found = match (seed.ch.as_deref(), seed.ink_box.as_ref()) {
                    (Some(ch), Some(ink)) if !ch.is_empty() => {
                        rail_for(outlines, first_char(ch), ink, x, y, angle)
                    }
                    _ => None,
                };
                match found {
                    Some(r) => {
                        growth_angle = r.tangent;
                        rail = Some(r);
                    }
                    None => {
                        let c = crate::geom::tangent_angles(x, y, &r);
                        growth_angle = if angle_diff(c[0], angle) <= angle_diff(c[1], angle) { c[0] } else { c[1] };
                    }
                }
            }

            Anchor { x, y, angle: growth_angle, size, rail }
        })
        .collect()
}

/// Outlines are keyed by single characters; a host that hands over a whole
/// mark gets its leading character looked up, which is the one a vine grows
/// off.
fn first_char(s: &str) -> &str {
    match s.char_indices().nth(1) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square glyph, in the unit square outlines are stored in.
    const SQUARE: &str = "M0,0 L1,0 L1,1 L0,1 Z";
    /// A square with a counter — an outer contour and a hole, like a β.
    const SQUARE_WITH_COUNTER: &str = "M0,0 L1,0 L1,1 L0,1 ZM0.4,0.4 L0.6,0.4 L0.6,0.6 L0.4,0.6 Z";

    fn outlines_with(ch: &str, d: &str) -> Outlines {
        let mut o = Outlines::new();
        o.insert(ch, d);
        o
    }

    #[test]
    fn a_rail_lands_on_the_glyphs_own_ink_box_not_the_unit_square() {
        let mut o = outlines_with("□", SQUARE);
        let ink = Rect::new(100.0, 50.0, 10.0, 20.0);
        let rail = rail_for(&mut o, "□", &ink, 100.0, 50.0, 0.0).expect("a square has a contour");
        for p in &rail.pts {
            assert!(p.x >= 99.9 && p.x <= 110.1, "x {} left the ink box", p.x);
            assert!(p.y >= 49.9 && p.y <= 70.1, "y {} left the ink box", p.y);
        }
        assert!((rail.cx - 105.0).abs() < 0.5 && (rail.cy - 60.0).abs() < 0.5, "centroid should be the box's own");
    }

    #[test]
    fn a_rail_never_rides_a_counter() {
        let mut o = outlines_with("β", SQUARE_WITH_COUNTER);
        let ink = Rect::new(0.0, 0.0, 100.0, 100.0);
        // Start right next to the counter: without the outer-contour filter,
        // "nearest contour wins" would pick the hole.
        let rail = rail_for(&mut o, "β", &ink, 50.0, 50.0, 0.0).expect("outline present");
        let on_counter = rail
            .pts
            .iter()
            .filter(|p| p.x > 39.0 && p.x < 61.0 && p.y > 39.0 && p.y < 61.0)
            .count();
        assert_eq!(on_counter, 0, "the vine would be crawling around inside the glyph");
    }

    #[test]
    fn an_unlisted_character_or_unmeasurable_ink_yields_no_rail() {
        let mut o = outlines_with("□", SQUARE);
        assert!(rail_for(&mut o, "ζ", &Rect::new(0.0, 0.0, 10.0, 10.0), 0.0, 0.0, 0.0).is_none());
        assert!(rail_for(&mut o, "□", &Rect::new(0.0, 0.0, 0.0, 10.0), 0.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn the_rail_direction_is_the_one_heading_outward() {
        let mut o = outlines_with("□", SQUARE);
        let ink = Rect::new(0.0, 0.0, 100.0, 100.0);
        let east = rail_for(&mut o, "□", &ink, 0.0, 0.0, 0.0).unwrap();
        let west = rail_for(&mut o, "□", &ink, 0.0, 0.0, std::f64::consts::PI).unwrap();
        assert_eq!(east.dir, 1, "the contour runs east out of its first sample");
        assert_eq!(west.dir, -1, "an outward heading the other way takes the other way round");
        assert!((east.tangent - west.tangent).abs() > 1.0, "and sets off in the opposite direction");
    }

    #[test]
    fn an_edge_anchor_sits_just_past_the_marks_own_box_facing_away_from_centre() {
        let host = Rect::new(0.0, 0.0, 200.0, 200.0);
        let seed = Seed {
            box_rect: Rect::new(150.0, 100.0, 10.0, 10.0),
            mark_rect: Some(Rect::new(150.0, 100.0, 10.0, 10.0)),
            mode: AnchorMode::Edge,
            ..Default::default()
        };
        let a = &resolve_anchors(&[seed], &host, &mut Outlines::new())[0];
        assert!(a.x > 160.0, "should sit outside the mark, on the side away from the host's centre");
        assert_eq!(a.size, 10.0, "size defaults to the mark rectangle's height");
    }

    #[test]
    fn a_composite_mark_is_measured_by_its_mark_rect_not_the_span() {
        // "β₃₂" spans much wider than the β; the anchor must answer to the
        // letter, not to the subscript riding it.
        let host = Rect::new(0.0, 0.0, 200.0, 200.0);
        let span = Rect::new(150.0, 100.0, 30.0, 12.0);
        let mark = Rect::new(150.0, 100.0, 10.0, 12.0);
        let with_mark = Seed { box_rect: span, mark_rect: Some(mark), mode: AnchorMode::Edge, ..Default::default() };
        let without = Seed { box_rect: span, mark_rect: None, mode: AnchorMode::Edge, ..Default::default() };
        let a = resolve_anchors(&[with_mark, without], &host, &mut Outlines::new());
        assert!(a[0].x < a[1].x, "the composite's own span pushes the anchor further out");
    }

    #[test]
    fn a_top_left_anchor_touches_its_corner_rather_than_floating_off_it() {
        let host = Rect::new(0.0, 0.0, 200.0, 200.0);
        let seed = Seed {
            box_rect: Rect::new(20.0, 30.0, 100.0, 60.0),
            mode: AnchorMode::TopLeft,
            size: Some(54.0),
            ..Default::default()
        };
        let a = &resolve_anchors(&[seed], &host, &mut Outlines::new())[0];
        assert_eq!((a.x, a.y), (20.0, 30.0));
        assert_eq!(a.size, 54.0, "an explicit size wins — a drop cap has no box of its own");
        assert!(a.rail.is_none(), "neither overridden mode carries a silhouette");
    }

    #[test]
    fn an_edge_anchor_with_an_outline_sets_off_along_the_letter() {
        let host = Rect::new(0.0, 0.0, 200.0, 200.0);
        let mut o = outlines_with("□", SQUARE);
        let r = Rect::new(150.0, 100.0, 10.0, 10.0);
        let seed = Seed {
            box_rect: r,
            mark_rect: Some(r),
            mode: AnchorMode::Edge,
            ch: Some("□".to_string()),
            ink_box: Some(r),
            ..Default::default()
        };
        let a = &resolve_anchors(&[seed], &host, &mut o)[0];
        let rail = a.rail.as_ref().expect("the square has an outline to ride");
        assert_eq!(a.angle, rail.tangent, "growth sets off along the contour, not off it");
    }

    #[test]
    fn first_char_takes_the_leading_character_of_a_composite_mark() {
        assert_eq!(first_char("¬⟨"), "¬");
        assert_eq!(first_char("⌘"), "⌘");
        assert_eq!(first_char(""), "");
    }
}
