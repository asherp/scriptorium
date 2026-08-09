// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Plain rectangle/segment geometry. Everything the walk asks about the world
// it grows through reduces to these: does this step cross that box, which way
// is parallel to its nearest edge, how far is this rectangle from that point.

use serde::{Deserialize, Serialize};

/// An axis-aligned rectangle in the host's own coordinate space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn center(&self) -> Point {
        Point { x: self.x + self.w / 2.0, y: self.y + self.h / 2.0 }
    }
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
    /// Grown by `pad` on every side — the halo an obstacle carries so growth
    /// clears the ink rather than grazing it.
    pub fn padded(&self, pad: f64) -> Rect {
        Rect { x: self.x - pad, y: self.y - pad, w: self.w + pad * 2.0, h: self.h + pad * 2.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
}

/// Segment/segment intersection (standard orientation test) — used to check a
/// growth step against a rectangle's four edges, not just its endpoints, so a
/// step that jumps clean over a thin obstacle can't sneak through undetected.
fn segments_cross(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
    let d = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let (d1, d2) = (d(p3, p4, p1), d(p3, p4, p2));
    let (d3, d4) = (d(p1, p2, p3), d(p1, p2, p4));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

pub fn segment_hits_rect(x1: f64, y1: f64, x2: f64, y2: f64, r: &Rect) -> bool {
    if r.contains(x1, y1) || r.contains(x2, y2) {
        return true;
    }
    let p1 = Point::new(x1, y1);
    let p2 = Point::new(x2, y2);
    let tl = Point::new(r.x, r.y);
    let tr = Point::new(r.right(), r.y);
    let bl = Point::new(r.x, r.bottom());
    let br = Point::new(r.right(), r.bottom());
    segments_cross(p1, p2, tl, tr)
        || segments_cross(p1, p2, tr, br)
        || segments_cross(p1, p2, br, bl)
        || segments_cross(p1, p2, bl, tl)
}

pub fn out_of_bounds(x: f64, y: f64, bounds: Option<&Rect>) -> bool {
    match bounds {
        None => false,
        Some(b) => x < b.x || x > b.right() || y < b.y || y > b.bottom(),
    }
}

/// The smallest signed angle between two headings, as a magnitude.
pub fn angle_diff(a: f64, b: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let mut d = (a - b) % two_pi;
    if d > std::f64::consts::PI {
        d -= two_pi;
    }
    if d < -std::f64::consts::PI {
        d += two_pi;
    }
    d.abs()
}

/// The two directions a turtle can travel while staying parallel to a
/// rectangle's nearest edge — i.e. tracing its silhouette rather than bouncing
/// off it. Whichever of the two the caller ends up choosing is what makes a
/// blocked vine curl along a paragraph's margin instead of deflecting away
/// from it at a random angle.
pub fn tangent_angles(px: f64, py: f64, rect: &Rect) -> [f64; 2] {
    let dist_top = (py - rect.y).abs();
    let dist_bottom = (py - rect.bottom()).abs();
    let dist_left = (px - rect.x).abs();
    let dist_right = (px - rect.right()).abs();
    let horizontal = dist_top.min(dist_bottom) <= dist_left.min(dist_right);
    let axis = if horizontal { 0.0 } else { std::f64::consts::FRAC_PI_2 };
    [axis, axis + std::f64::consts::PI]
}

/// Whether a rectangle comes within `radius` of a point — the standard
/// distance from a point to an axis-aligned box, zero on each axis the point
/// already falls within.
///
/// Used to hand each anchor only the obstacles its own leash could ever bring
/// it near: a page's worth of term boxes runs into the thousands, and every
/// one of them would otherwise be tested against every step of every walk.
pub fn rect_near(r: &Rect, x: f64, y: f64, radius: f64) -> bool {
    let dx = (r.x - x).max(0.0).max(x - r.right());
    let dy = (r.y - y).max(0.0).max(y - r.bottom());
    dx * dx + dy * dy <= radius * radius
}

/// Where a ray from a rectangle's own center, heading in `angle`, crosses the
/// rectangle's boundary — the standard center-to-edge intersection, taking
/// whichever of the two axis distances the ray reaches first.
pub fn edge_point(r: &Rect, angle: f64) -> Point {
    let c = r.center();
    let (dx, dy) = (angle.cos(), angle.sin());
    let tx = if dx != 0.0 { (r.w / 2.0) / dx.abs() } else { f64::INFINITY };
    let ty = if dy != 0.0 { (r.h / 2.0) / dy.abs() } else { f64::INFINITY };
    let t = tx.min(ty);
    Point::new(c.x + dx * t, c.y + dy * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_jumping_clean_over_a_thin_obstacle_still_counts_as_blocked() {
        // Neither endpoint is inside; only the edge crossings catch this.
        let sliver = Rect::new(10.0, -50.0, 1.0, 100.0);
        assert!(segment_hits_rect(0.0, 0.0, 20.0, 0.0, &sliver));
    }

    #[test]
    fn a_step_that_misses_entirely_is_free() {
        let far = Rect::new(500.0, 500.0, 50.0, 50.0);
        assert!(!segment_hits_rect(0.0, 0.0, 20.0, 0.0, &far));
    }

    #[test]
    fn tangents_run_along_whichever_edge_is_nearest() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        // Just above the top edge: the nearest edge is horizontal.
        let t = tangent_angles(50.0, -1.0, &r);
        assert_eq!(t[0], 0.0);
        // Just left of the left edge: the nearest edge is vertical.
        let t = tangent_angles(-1.0, 50.0, &r);
        assert_eq!(t[0], std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn rect_near_measures_from_the_box_not_its_centre() {
        let r = Rect::new(100.0, 0.0, 10.0, 10.0);
        assert!(rect_near(&r, 95.0, 5.0, 6.0), "5px away on one axis, inside on the other");
        assert!(!rect_near(&r, 80.0, 5.0, 6.0));
    }

    #[test]
    fn no_bounds_means_nothing_is_ever_out_of_them() {
        assert!(!out_of_bounds(1e9, -1e9, None));
    }
}
