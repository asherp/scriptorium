// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Outer contours: the letterform's silhouette, not its holes.
//
// A vine rides the OUTSIDE of a glyph. Plenty of marks are made of several
// subpaths — a circled zero has four, a place-of-interest sign six, a β an
// outer contour and two counters — and a counter is a hole in the letter, not
// an edge of it: growth that rode one would be crawling around inside the
// glyph. So a contour that lies within a larger one is dropped, and what
// remains is the silhouette.
//
// Note the plural: plenty of marks are genuinely several separate outer shapes
// (`=` is two bars, `‖` two strokes, `|·|` three), none of which contains the
// others, and all of which are legitimately rideable — so this discards the
// enclosed, never all but the biggest.

use crate::geom::Point;

/// Signed shoelace area. The font's winding decides the sign, and only the
/// magnitude is ever used.
fn contour_area(pts: &[Point]) -> f64 {
    let mut a = 0.0;
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut j = n - 1;
    for i in 0..n {
        a += (pts[j].x + pts[i].x) * (pts[j].y - pts[i].y);
        j = i;
    }
    a / 2.0
}

/// Standard crossing-number test, on the sampled polyline rather than the
/// curve it came from — a couple of hundred samples of a letterform is far
/// finer than the distinction being drawn here (is this shape inside that
/// one?) needs.
pub fn point_in_contour(p: Point, pts: &[Point]) -> bool {
    let mut inside = false;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (pts[i], pts[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Drops every contour that some LARGER contour contains.
///
/// Comparing areas is what keeps a pair of contours that (through sampling
/// noise on a shared edge) each appear to contain the other from eliminating
/// both. If the filter would leave nothing at all, the input is returned
/// untouched: a vine with no rail is better served by a wrong contour than by
/// none.
pub fn outer_contours(contours: Vec<Vec<Point>>) -> Vec<Vec<Point>> {
    if contours.len() < 2 {
        return contours;
    }
    let areas: Vec<f64> = contours.iter().map(|c| contour_area(c).abs()).collect();
    let keep: Vec<usize> = (0..contours.len())
        .filter(|&i| {
            let first = match contours[i].first() {
                Some(p) => *p,
                None => return false,
            };
            !(0..contours.len())
                .any(|j| j != i && areas[j] > areas[i] && point_in_contour(first, &contours[j]))
        })
        .collect();
    if keep.is_empty() {
        return contours;
    }
    let mut out = Vec::with_capacity(keep.len());
    for (i, c) in contours.into_iter().enumerate() {
        if keep.contains(&i) {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square stands in for a letterform: the containment question is the
    /// same shape whatever the curve.
    fn square(x: f64, y: f64, size: f64) -> Vec<Point> {
        let n = 20;
        let mut pts = Vec::new();
        for i in 0..n {
            pts.push(Point::new(x + size * (i as f64 / n as f64), y));
        }
        for i in 0..n {
            pts.push(Point::new(x + size, y + size * (i as f64 / n as f64)));
        }
        for i in 0..n {
            pts.push(Point::new(x + size - size * (i as f64 / n as f64), y + size));
        }
        for i in 0..n {
            pts.push(Point::new(x, y + size - size * (i as f64 / n as f64)));
        }
        pts
    }

    #[test]
    fn drops_a_counter_and_keeps_the_silhouette() {
        let outer = square(0.0, 0.0, 100.0);
        let counter = square(30.0, 30.0, 40.0); // a hole well inside it — β's bowl, ⓪'s zero
        let kept = outer_contours(vec![outer.clone(), counter]);
        assert_eq!(kept.len(), 1, "the counter should not be a candidate for a vine to ride");
        assert_eq!(kept[0], outer);
    }

    #[test]
    fn keeps_every_separate_outer_shape() {
        // `=` is two bars, `‖` two strokes, `|·|` three: none contains the
        // others, and a vine growing off one of them rides that one. Keeping
        // only the largest would strand growth on the wrong stroke.
        let bars = vec![square(0.0, 0.0, 30.0), square(0.0, 60.0, 28.0), square(0.0, 120.0, 26.0)];
        assert_eq!(outer_contours(bars).len(), 3);
    }

    #[test]
    fn leaves_a_single_contour_alone_and_never_eliminates_everything() {
        let one = square(0.0, 0.0, 50.0);
        assert_eq!(outer_contours(vec![one.clone()]), vec![one.clone()]);
        // Two identical contours each "contain" the other; neither is larger,
        // so the area comparison keeps both rather than discarding the pair.
        assert_eq!(outer_contours(vec![one.clone(), one]).len(), 2);
    }
}
