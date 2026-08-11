// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The silhouette a block of text presents to the rest of the page.
//
// A scribe ruling a border does not trace every ragged line-end of the
// paragraph it encloses; the border is the shape the block makes seen from
// outside, and the ragged ends are inside it. That shape is the convex hull of
// the block's own LETTERS — their outlines, not their boxes — and it is the
// second thing in this engine a vine can ride, the first being a single
// letterform's contour.
//
// Letters rather than boxes, because a box is not a shape a reader sees. An
// inline box carries the line's leading above and below the letter and the
// advance's side bearings either side of it, so a hull over boxes is a hull
// over whitespace: it squares off at the tallest LINE rather than at the
// tallest ascender, and a block of small caps and a block of parentheses wrap
// identically. Over outlines, the ring answers to the writing — it rises over
// an ascender, dips under a descender, and cuts the corner where a line ends
// short.
//
// Two properties are load-bearing, and both are tested here rather than
// assumed by the walk:
//
//   - The hull is wound COUNTER-CLOCKWISE as the page is read. The host's
//     space has y pointing DOWN, so counter-clockwise on screen is the
//     NEGATIVE signed area, not the positive one every textbook quotes. Get it
//     backwards and every vine in the margin runs the wrong way round the
//     block.
//   - The hull CLEARS the ink it wraps. The ring is the letters' hull grown by
//     the obstacle halo plus a margin, so every glyph sits strictly inside it.
//     A ride along a hull that grazed the text would be blocked on its first
//     step and there would be no border at all.

use serde::{Deserialize, Serialize};

use crate::anchor::{first_char, Outlines};
use crate::geom::{Point, Rect};
use crate::turtle::Rail;

/// One rendered character of a block, as the host measured it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Glyph {
    /// The character itself, which is what an outline is looked up by. One
    /// with no outline registered contributes its ink box's own corners, so a
    /// host that has not sampled its font still gets a silhouette — just a
    /// blockier one.
    pub ch: Option<String>,
    /// Where that character puts ink, in the host's own space: the box the
    /// unit-square outline maps onto, exactly as for a seed's own mark.
    #[serde(rename = "box")]
    pub box_rect: Rect,
}

impl Glyph {
    pub fn new(ch: &str, box_rect: Rect) -> Self {
        Glyph { ch: Some(ch.to_string()), box_rect }
    }
}

/// The silhouette of one block, wound counter-clockwise as the reader sees it
/// and grown by `pad` on every side.
///
/// `pad` is what buys the ride its clearance: pass the obstacle halo plus a
/// margin and the ring is guaranteed to stay clear of every glyph it wraps.
///
/// Fewer than three distinct points — an empty block, or one whose glyphs are
/// degenerate — yields whatever there was, which the rail builder then
/// declines to ride.
pub fn block_hull(glyphs: &[Glyph], outlines: &mut Outlines, pad: f64) -> Vec<Point> {
    let mut pts = Vec::with_capacity(glyphs.len() * 12);
    for g in glyphs {
        let b = g.box_rect;
        // A host's measurement can arrive as NaN — a font that hasn't loaded,
        // an element that isn't laid out. One such point would swallow the
        // whole hull, so it is dropped rather than sorted.
        if !(b.w >= 0.0) || !(b.h >= 0.0) || !b.x.is_finite() || !b.y.is_finite() {
            continue;
        }
        let unit = g
            .ch
            .as_deref()
            .filter(|c| !c.is_empty())
            .and_then(|c| outlines.unit_hull(first_char(c)));
        match unit {
            Some(hull) => {
                pts.extend(hull.iter().map(|p| Point::new(b.x + p.x * b.w, b.y + p.y * b.h)));
            }
            None => pts.extend(corners(&b)),
        }
    }
    // Hull first, THEN grow: growing a convex set commutes with hulling it
    // (a Minkowski sum does), so this is the same ring as growing every one of
    // the thousands of glyph points would give, for the price of growing a
    // handful.
    grown(convex_hull(pts), pad)
}

fn corners(r: &Rect) -> [Point; 4] {
    [
        Point::new(r.x, r.y),
        Point::new(r.right(), r.y),
        Point::new(r.right(), r.bottom()),
        Point::new(r.x, r.bottom()),
    ]
}

/// The ring grown by `pad` on every side — every point replaced by the corners
/// of the square it would sweep, and the result re-hulled.
fn grown(ring: Vec<Point>, pad: f64) -> Vec<Point> {
    if !(pad > 0.0) || ring.len() < 3 {
        return ring;
    }
    let mut pts = Vec::with_capacity(ring.len() * 4);
    for p in &ring {
        pts.extend(corners(&Rect::new(p.x - pad, p.y - pad, pad * 2.0, pad * 2.0)));
    }
    convex_hull(pts)
}

/// Andrew's monotone chain, wound counter-clockwise as the page is read.
///
/// Returns the ring with no repeated closing point. Fewer than three distinct
/// points come back as they were: there is no hull of a line or of a dot, and
/// pretending otherwise would only move the problem downstream.
pub fn convex_hull(mut pts: Vec<Point>) -> Vec<Point> {
    pts.retain(|p| p.x.is_finite() && p.y.is_finite());
    pts.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    pts.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if pts.len() < 3 {
        return pts;
    }

    let cross = |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let chain = |iter: &mut dyn Iterator<Item = &Point>| -> Vec<Point> {
        let mut out: Vec<Point> = Vec::new();
        for &p in iter {
            while out.len() >= 2 && cross(out[out.len() - 2], out[out.len() - 1], p) <= 0.0 {
                out.pop();
            }
            out.push(p);
        }
        out.pop(); // the shared endpoint belongs to the other half
        out
    };

    let mut hull = chain(&mut pts.iter());
    hull.extend(chain(&mut pts.iter().rev()));
    if signed_area2(&hull) > 0.0 {
        hull.reverse();
    }
    hull
}

/// Twice the signed area of a ring in the host's y-DOWN space: NEGATIVE for a
/// ring wound counter-clockwise as the reader sees it.
fn signed_area2(ring: &[Point]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// A ring with a point at least every `spacing` along it.
///
/// A hull has a handful of vertices and edges hundreds of pixels long, and the
/// walk advances along a rail by accumulating real distance from one sample to
/// the next — handed the bare vertices it would cross a whole side of the
/// paragraph in a single step. Sampling is what makes a ride along a block the
/// same kind of motion as a ride along a letter.
fn densify(ring: &[Point], spacing: f64) -> Vec<Point> {
    if ring.len() < 3 || !(spacing > 0.0) {
        return ring.to_vec();
    }
    let n = ring.len();
    let mut out = Vec::with_capacity(n * 8);
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let len = (b.x - a.x).hypot(b.y - a.y);
        out.push(a);
        // Round UP: `spacing` is the widest gap the walk should ever meet, not
        // the narrowest one worth inserting.
        let cuts = (len / spacing).ceil().max(1.0) as usize;
        for k in 1..cuts {
            let t = k as f64 / cuts as f64;
            out.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    out
}

/// The signed angle from `from` to `to`, wrapped into (-π, π].
fn wrapped(to: f64, from: f64) -> f64 {
    let mut d = to - from;
    while d > std::f64::consts::PI {
        d -= std::f64::consts::TAU;
    }
    while d <= -std::f64::consts::PI {
        d += std::f64::consts::TAU;
    }
    d
}

/// The rail a vine rides around a block, or `None` where there is no ring to
/// ride.
///
/// Growth begins at the mark's CLOCKWISE-MOST extent and travels the ring the
/// way it was wound, which is counter-clockwise. That pairing is the whole
/// point: start anywhere else on the mark and the ride sets off part-way
/// across the very letter it grew from, leaving the rest of that letter's own
/// stretch of ring behind it and never coming back. Starting at the clockwise
/// end, the first thing the vine does is wrap the mark, and only then does it
/// carry on round the block.
///
/// Clockwise-most is read as an angle about the ring's own centre, measured
/// against the anchor so a mark sitting where the angle wraps is no special
/// case. A convex ring wound counter-clockwise on a y-DOWN screen travels in
/// the direction of DECREASING angle, so the corner to start from is the one
/// with the greatest angle: everything else on the mark lies ahead of it.
///
/// The direction itself is not re-derived here the way [`crate::rail_for`]
/// derives it for a letter: a letterform is a shape with an inside and an
/// outside and no natural sense of travel, whereas a block's silhouette was
/// wound counter-clockwise on purpose, and that decision is the host's to keep.
pub fn rail_along(hull: &[Point], spacing: f64, mark: &Rect, x: f64, y: f64) -> Option<Rail> {
    let pts = densify(hull, spacing);
    let n = pts.len();
    if n < 3 {
        return None;
    }

    let (mut cx, mut cy) = (0.0, 0.0);
    for p in &pts {
        cx += p.x;
        cy += p.y;
    }
    let (cx, cy) = (cx / n as f64, cy / n as f64);

    // The point on the mark to begin from, then the ring sample nearest it.
    let base = (y - cy).atan2(x - cx);
    let mut from = Point::new(x, y);
    let mut most = f64::NEG_INFINITY;
    for c in corners(mark) {
        let rel = wrapped((c.y - cy).atan2(c.x - cx), base);
        if rel > most {
            most = rel;
            from = c;
        }
    }

    let mut start_idx = 0;
    let mut best = f64::INFINITY;
    for (i, p) in pts.iter().enumerate() {
        let d = (p.x - from.x).powi(2) + (p.y - from.y).powi(2);
        if d < best {
            best = d;
            start_idx = i;
        }
    }

    let at = |i: usize| pts[i % n];
    let tangent = (at(start_idx + 2).y - at(start_idx).y).atan2(at(start_idx + 2).x - at(start_idx).x);
    Some(Rail { pts, start_idx, dir: 1, tangent, cx, cy })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square glyph filling its whole ink box.
    const SQUARE: &str = "M0,0 L1,0 L1,1 L0,1 Z";
    /// A glyph whose ink is a bar across the BOTTOM of its box, like the
    /// underscore an outline-less measurement can never tell apart from a `l`.
    const LOW_BAR: &str = "M0,0.85 L1,0.85 L1,1 L0,1 Z";

    /// Three lines of a justified paragraph, the last one short, as boxes.
    fn boxes() -> Vec<Rect> {
        vec![
            Rect::new(0.0, 0.0, 300.0, 14.0),
            Rect::new(0.0, 20.0, 300.0, 14.0),
            Rect::new(0.0, 40.0, 120.0, 14.0),
        ]
    }

    /// The same three lines, as glyphs with no outline to look up.
    fn unlettered() -> Vec<Glyph> {
        boxes().into_iter().map(|box_rect| Glyph { ch: None, box_rect }).collect()
    }

    fn outlines_with(ch: &str, d: &str) -> Outlines {
        let mut o = Outlines::new();
        o.insert(ch, d);
        o
    }

    #[test]
    fn a_block_is_wrapped_counter_clockwise_as_the_page_is_read() {
        let hull = block_hull(&unlettered(), &mut Outlines::new(), 0.0);
        assert!(signed_area2(&hull) < 0.0, "y points down, so counter-clockwise is negative");

        // Concretely: leaving the top-left corner, the next point round the
        // ring is further down the page, not further across it.
        let top_left = hull
            .iter()
            .position(|p| p.x == 0.0 && p.y == 0.0)
            .expect("the block's own corner is on its hull");
        let next = hull[(top_left + 1) % hull.len()];
        assert!(next.y > 0.0 && next.x == 0.0, "counter-clockwise leaves the top-left going DOWN");
    }

    #[test]
    fn the_ragged_end_of_a_short_line_is_inside_the_silhouette() {
        let hull = block_hull(&unlettered(), &mut Outlines::new(), 0.0);
        // Five corners: the block's own, plus the two ends of the single
        // slanted edge spanning the notch the short last line leaves. The
        // notch itself is inside the silhouette, never traced.
        assert_eq!(hull.len(), 5);
        assert!(hull.iter().any(|p| p.x == 300.0 && p.y == 34.0), "the long lines' right edge");
        assert!(hull.iter().any(|p| p.x == 120.0 && p.y == 54.0), "the short line's own end");
        assert!(
            !hull.iter().any(|p| p.x == 300.0 && p.y == 54.0),
            "nothing is invented where the text does not reach"
        );
    }

    #[test]
    fn the_ring_answers_to_the_letters_rather_than_to_their_boxes() {
        // One line of ink sitting low in its box: over boxes the silhouette
        // squares off at the top of the line, over letters it comes down to
        // where the writing actually is.
        let mut o = outlines_with("_", LOW_BAR);
        let line = Rect::new(0.0, 0.0, 100.0, 20.0);
        let lettered = block_hull(&[Glyph::new("_", line)], &mut o, 0.0);
        let boxed = block_hull(&[Glyph { ch: None, box_rect: line }], &mut o, 0.0);

        let top = |h: &[Point]| h.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        assert_eq!(top(&boxed), 0.0, "a box reaches the top of the line whatever is in it");
        assert!((top(&lettered) - 17.0).abs() < 0.5, "the ink starts at 0.85 of the box");
        assert!(top(&lettered) > top(&boxed), "letters wrap tighter than the boxes they sit in");
    }

    #[test]
    fn a_letter_with_no_outline_still_wraps_by_its_box() {
        // Mixed: a character the host sampled, and one it didn't.
        let mut o = outlines_with("□", SQUARE);
        let hull = block_hull(
            &[
                Glyph::new("□", Rect::new(0.0, 0.0, 10.0, 10.0)),
                Glyph::new("?", Rect::new(20.0, 0.0, 10.0, 10.0)),
            ],
            &mut o,
            0.0,
        );
        assert!(hull.iter().any(|p| p.x == 30.0), "the unsampled letter is wrapped all the same");
    }

    #[test]
    fn the_ring_clears_the_ink_it_wraps() {
        // The invariant the ride depends on: grown by the halo PLUS a margin,
        // no point of the ring lies inside a glyph's own padded box.
        let pad = 2.5;
        let margin = 6.0;
        let hull = block_hull(&unlettered(), &mut Outlines::new(), pad + margin);
        for r in boxes() {
            let padded = r.padded(pad);
            for p in &hull {
                assert!(!padded.contains(p.x, p.y), "the border {p:?} runs through {padded:?}");
            }
        }
    }

    #[test]
    fn a_ride_starts_on_the_stretch_of_ring_its_own_mark_is_exposed_along() {
        let hull = block_hull(&unlettered(), &mut Outlines::new(), 4.0);
        // A mark opening the last line, against the block's left edge.
        let mark = Rect::new(0.0, 40.0, 9.0, 14.0);
        let rail = rail_along(&hull, 2.0, &mark, -4.0, 47.0).expect("a hull is rideable");
        let start = rail.pts[rail.start_idx];
        assert!((start.x - -4.0).abs() < 2.5, "the ride starts on the edge the mark touches");
        assert!((start.y - 40.0).abs() < 2.5, "and at the mark's clockwise end, its top");
        assert_eq!(rail.dir, 1, "the ring is ridden in the direction it was wound");
    }

    #[test]
    fn a_ride_wraps_its_own_mark_before_carrying_on_round_the_block() {
        let hull = block_hull(&unlettered(), &mut Outlines::new(), 4.0);
        let mark = Rect::new(0.0, 40.0, 9.0, 14.0);
        let rail = rail_along(&hull, 2.0, &mark, -4.0, 47.0).expect("a hull is rideable");
        // Walking the ring forward from the start, the mark's whole vertical
        // extent is covered before the ride leaves it — which is what starting
        // at the clockwise end buys, and what starting at the nearest point
        // (the mark's middle, y 47) would have thrown away.
        let n = rail.pts.len();
        let first_20: Vec<Point> = (0..20).map(|k| rail.pts[(rail.start_idx + k) % n]).collect();
        let top = first_20.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let bottom = first_20.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        assert!(top <= mark.y + 1.0, "the ride covers the mark's top");
        assert!(bottom >= mark.bottom() - 1.0, "and goes on past its foot");
    }

    #[test]
    fn a_ride_down_the_left_edge_heads_down_the_page() {
        let hull = block_hull(&unlettered(), &mut Outlines::new(), 4.0);
        let mark = Rect::new(0.0, 14.0, 9.0, 14.0);
        let rail = rail_along(&hull, 2.0, &mark, -4.0, 20.0).expect("a hull is rideable");
        // Counter-clockwise, a mark part-way down the left edge sets off
        // toward the foot of the block.
        assert!(rail.tangent.sin() > 0.0, "y grows downward, so a downward heading has sin > 0");
        assert!(rail.tangent.cos().abs() < 0.2, "and it runs along the edge, not across it");
    }

    #[test]
    fn a_mark_where_the_angle_wraps_is_no_special_case() {
        // A block to the RIGHT of its own mark puts the mark at the ±pi seam,
        // where a naive angle comparison picks the wrong corner.
        let hull = block_hull(&unlettered(), &mut Outlines::new(), 4.0);
        let mark = Rect::new(0.0, 20.0, 9.0, 14.0);
        let rail = rail_along(&hull, 2.0, &mark, -4.0, 27.0).expect("a hull is rideable");
        let start = rail.pts[rail.start_idx];
        assert!((start.y - 20.0).abs() < 2.5, "the seam does not move the start off the mark's top");
    }

    #[test]
    fn sampling_puts_a_step_within_reach_all_the_way_round() {
        let hull = block_hull(&unlettered(), &mut Outlines::new(), 4.0);
        let rail = rail_along(&hull, 2.0, &Rect::new(0.0, 0.0, 4.0, 4.0), 0.0, 0.0)
            .expect("a hull is rideable");
        for i in 0..rail.pts.len() {
            let (a, b) = (rail.pts[i], rail.pts[(i + 1) % rail.pts.len()]);
            assert!((b.x - a.x).hypot(b.y - a.y) <= 2.0 + 1e-9, "a gap the walk would jump");
        }
    }

    #[test]
    fn a_block_with_nothing_measurable_in_it_is_not_rideable() {
        let mut o = Outlines::new();
        assert!(block_hull(&[], &mut o, 4.0).is_empty());
        assert!(rail_along(&[], 2.0, &Rect::new(0.0, 0.0, 4.0, 4.0), 0.0, 0.0).is_none());
        let degenerate = [
            Glyph { ch: None, box_rect: Rect::new(f64::NAN, 0.0, 10.0, 10.0) },
            Glyph { ch: None, box_rect: Rect::new(0.0, 0.0, f64::NAN, 10.0) },
        ];
        assert!(block_hull(&degenerate, &mut o, 4.0).len() < 3);
        // One real letter still wraps: a lone mark is a block of its own.
        let one = block_hull(&[Glyph { ch: None, box_rect: Rect::new(10.0, 10.0, 8.0, 12.0) }], &mut o, 1.0);
        assert_eq!(one.len(), 4);
        assert!(signed_area2(&one) < 0.0);
    }
}
