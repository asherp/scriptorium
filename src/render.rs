// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Turning a finished walk into SVG path data.
//
// Only the `d`/`transform` strings are produced here, never elements: what
// stroke, what opacity, what class a host paints them with is the host's own
// affair — the reference host draws them on `currentColor` so the decoration
// inherits the page's text colour with no stylesheet of its own.

use serde::{Deserialize, Serialize};

use crate::geom::Point;
use crate::turtle::{Leaf, Segment};

/// A leaf's own geometry: two path strings and the transform that places them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeafPath {
    /// The blade outline, to be filled.
    pub blade: String,
    /// The midrib, to be stroked.
    pub vein: String,
    /// Places both at the vine, pointing along the leaf's own heading.
    pub transform: String,
}

/// One segment, ready to draw.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SegmentPath {
    /// The vine's own path data, empty for a segment that is only leaves.
    pub d: String,
    pub leaves: Vec<LeafPath>,
}

/// JavaScript's `Number.prototype.toFixed`, near enough: round half away from
/// zero at `digits` decimals, then print exactly that many.
///
/// Path data is where this engine's output meets a host's DOM, and matching
/// the reference implementation's rounding keeps a port's output diffable
/// against it rather than merely equivalent.
pub(crate) fn fixed(v: f64, digits: usize) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let f = 10f64.powi(digits as i32);
    let r = (v * f).round() / f;
    // -0 prints as "0" rather than "-0.0": a negative zero coordinate is a
    // rounding artefact, not a direction.
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r:.digits$}")
}

/// A polyline as SVG path data.
pub fn path_d(points: &[Point]) -> String {
    let mut out = String::with_capacity(points.len() * 14);
    for (i, p) in points.iter().enumerate() {
        out.push(if i == 0 { 'M' } else { 'L' });
        out.push_str(&fixed(p.x, 1));
        out.push(',');
        out.push_str(&fixed(p.y, 1));
        if i + 1 < points.len() {
            out.push(' ');
        }
    }
    out
}

/// A small heart/arrowhead leaf, attached at the vine by its base (the local
/// origin) and pointing along the leaf's angle — two rounded lobes bulging out
/// near the attachment point, tapering to a single tip, the shape common to
/// the bindweed/morning-glory-style cordate leaves this was drawn from. A
/// faint midrib is drawn as a second, thinner stroke down the same shape,
/// since every leaf in those references shows one.
pub fn leaf_path(leaf: &Leaf) -> LeafPath {
    let deg = leaf.angle.to_degrees();
    let scale = if leaf.scale.is_finite() && leaf.scale != 0.0 { leaf.scale } else { 1.0 };
    LeafPath {
        blade: "M0,0 C1.5,-2.8 5,-3.8 7,-2 Q9,-0.4 9.4,0 Q9,0.4 7,2 C5,3.8 1.5,2.8 0,0 Z".to_string(),
        vein: "M0.6,0 Q5,0 8.6,0".to_string(),
        transform: format!(
            "translate({},{}) rotate({}) scale({})",
            fixed(leaf.x, 1),
            fixed(leaf.y, 1),
            fixed(deg, 1),
            fixed(scale, 2)
        ),
    }
}

/// Every segment of one anchor's walk, as path data.
pub fn segment_paths(segments: &[Segment]) -> Vec<SegmentPath> {
    segments
        .iter()
        .map(|seg| SegmentPath {
            d: if seg.points.len() > 1 { path_d(&seg.points) } else { String::new() },
            leaves: seg.leaves.iter().map(leaf_path).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_polyline_opens_with_a_moveto_and_continues_with_linetos() {
        let d = path_d(&[Point::new(0.0, 0.0), Point::new(7.0, 0.0), Point::new(7.04, 3.25)]);
        assert_eq!(d, "M0.0,0.0 L7.0,0.0 L7.0,3.3");
    }

    #[test]
    fn coordinates_are_rounded_to_one_decimal_and_never_print_negative_zero() {
        assert_eq!(fixed(-0.04, 1), "0.0");
        assert_eq!(fixed(2.05, 1), "2.1");
        assert_eq!(fixed(-2.05, 1), "-2.1");
        assert_eq!(fixed(f64::NAN, 1), "0");
    }

    #[test]
    fn a_leaf_is_placed_by_transform_rather_than_by_redrawing_its_blade() {
        let a = leaf_path(&Leaf { x: 10.0, y: 20.0, angle: std::f64::consts::PI, scale: 1.25 });
        let b = leaf_path(&Leaf { x: -3.0, y: 4.5, angle: 0.0, scale: 1.0 });
        assert_eq!(a.blade, b.blade, "every leaf is the same blade, moved");
        assert_eq!(a.transform, "translate(10.0,20.0) rotate(180.0) scale(1.25)");
        assert_eq!(b.transform, "translate(-3.0,4.5) rotate(0.0) scale(1.00)");
    }

    #[test]
    fn a_leafless_single_point_segment_yields_no_path_data_to_draw() {
        let segs = vec![Segment { points: vec![Point::new(1.0, 1.0)], leaves: vec![] }];
        assert_eq!(segment_paths(&segs)[0].d, "");
    }
}
