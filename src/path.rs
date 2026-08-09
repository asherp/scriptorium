// SPDX-License-Identifier: MIT OR Apache-2.0
//
// SVG path data -> polylines, sampled by arc length.
//
// This is the one thing the engine used to borrow a browser for: an
// `SVGPathElement`'s `getTotalLength()` / `getPointAtLength()` are exactly the
// "walk this outline at even intervals" primitive a rail needs. Doing it here
// instead is what lets the whole engine run — and be tested — with no DOM at
// all, and costs only a flattener and a cumulative-length table.
//
// A path is split at each `M` first, since a letter is rarely one closed curve
// (a β has an outer contour and two counters) and sampling straight across the
// jump between subpaths would invent a segment that crosses the letter.
//
// Arcs (`A`/`a`) are not supported: the glyph outlines this engine is handed
// are quadratic/cubic contours, which is what font outlines are made of.

use crate::geom::Point;

/// How finely each curve segment is flattened before arc-length sampling.
/// A glyph contour resampled at ~220 points cannot see the difference beyond
/// this, and every consumer downstream is asking a question (which way does
/// this edge run, is this shape inside that one) far coarser still.
const FLATTEN_STEPS: usize = 16;

struct Lexer<'a> {
    rest: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(s: &'a str) -> Self {
        Lexer { rest: s }
    }
    fn skip_sep(&mut self) {
        self.rest = self.rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    }
    fn command(&mut self) -> Option<char> {
        self.skip_sep();
        let c = self.rest.chars().next()?;
        if c.is_ascii_alphabetic() {
            self.rest = &self.rest[c.len_utf8()..];
            Some(c)
        } else {
            None
        }
    }
    fn number(&mut self) -> Option<f64> {
        self.skip_sep();
        let bytes = self.rest.as_bytes();
        let mut i = 0;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let mut seen_digit = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            seen_digit = true;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
                seen_digit = true;
            }
        }
        if !seen_digit {
            return None;
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            let mut j = i + 1;
            if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                j += 1;
            }
            let mut exp_digit = false;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
                exp_digit = true;
            }
            if exp_digit {
                i = j;
            }
        }
        let (num, rest) = self.rest.split_at(i);
        self.rest = rest;
        num.parse().ok()
    }
    fn at_end(&mut self) -> bool {
        self.skip_sep();
        self.rest.is_empty()
    }
    fn peek_is_number(&mut self) -> bool {
        self.skip_sep();
        match self.rest.chars().next() {
            Some(c) => c.is_ascii_digit() || c == '-' || c == '+' || c == '.',
            None => false,
        }
    }
}

fn push_pt(out: &mut Vec<Point>, p: Point) {
    // A repeated coordinate contributes no length and would only put a
    // zero-length rung in the cumulative table.
    if let Some(last) = out.last() {
        if (last.x - p.x).abs() < 1e-12 && (last.y - p.y).abs() < 1e-12 {
            return;
        }
    }
    out.push(p);
}

fn flatten_quad(out: &mut Vec<Point>, p0: Point, c: Point, p1: Point) {
    for i in 1..=FLATTEN_STEPS {
        let t = i as f64 / FLATTEN_STEPS as f64;
        let mt = 1.0 - t;
        push_pt(
            out,
            Point::new(
                mt * mt * p0.x + 2.0 * mt * t * c.x + t * t * p1.x,
                mt * mt * p0.y + 2.0 * mt * t * c.y + t * t * p1.y,
            ),
        );
    }
}

fn flatten_cubic(out: &mut Vec<Point>, p0: Point, c1: Point, c2: Point, p1: Point) {
    for i in 1..=FLATTEN_STEPS {
        let t = i as f64 / FLATTEN_STEPS as f64;
        let mt = 1.0 - t;
        let (a, b, c, d) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
        push_pt(
            out,
            Point::new(
                a * p0.x + b * c1.x + c * c2.x + d * p1.x,
                a * p0.y + b * c1.y + c * c2.y + d * p1.y,
            ),
        );
    }
}

/// Flattens one subpath's commands into a polyline. Returns `None` for a
/// subpath that never puts down a point.
fn flatten_subpath(d: &str) -> Option<Vec<Point>> {
    let mut lex = Lexer::new(d);
    let mut out: Vec<Point> = Vec::new();
    let mut cur = Point::new(0.0, 0.0);
    let mut start = Point::new(0.0, 0.0);
    // The reflected control point `S`/`T` continue from, when the previous
    // command was of the matching kind.
    let mut last_cubic_ctrl: Option<Point> = None;
    let mut last_quad_ctrl: Option<Point> = None;
    let mut cmd = ' ';

    while !lex.at_end() {
        if let Some(c) = lex.command() {
            cmd = c;
        } else if cmd == 'M' {
            // Repeated coordinate pairs after an M are implicit L's.
            cmd = 'L';
        } else if cmd == 'm' {
            cmd = 'l';
        } else if !lex.peek_is_number() {
            break;
        }

        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cur.x, cur.y) } else { (0.0, 0.0) };

        match cmd.to_ascii_uppercase() {
            'M' => {
                let (x, y) = (lex.number()? + ox, lex.number()? + oy);
                cur = Point::new(x, y);
                start = cur;
                push_pt(&mut out, cur);
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            'L' => {
                let (x, y) = (lex.number()? + ox, lex.number()? + oy);
                cur = Point::new(x, y);
                push_pt(&mut out, cur);
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            'H' => {
                let x = lex.number()? + ox;
                cur = Point::new(x, cur.y);
                push_pt(&mut out, cur);
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            'V' => {
                let y = lex.number()? + oy;
                cur = Point::new(cur.x, y);
                push_pt(&mut out, cur);
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            'Q' => {
                let c = Point::new(lex.number()? + ox, lex.number()? + oy);
                let p = Point::new(lex.number()? + ox, lex.number()? + oy);
                flatten_quad(&mut out, cur, c, p);
                cur = p;
                last_quad_ctrl = Some(c);
                last_cubic_ctrl = None;
            }
            'T' => {
                let c = match last_quad_ctrl {
                    Some(prev) => Point::new(2.0 * cur.x - prev.x, 2.0 * cur.y - prev.y),
                    None => cur,
                };
                let p = Point::new(lex.number()? + ox, lex.number()? + oy);
                flatten_quad(&mut out, cur, c, p);
                cur = p;
                last_quad_ctrl = Some(c);
                last_cubic_ctrl = None;
            }
            'C' => {
                let c1 = Point::new(lex.number()? + ox, lex.number()? + oy);
                let c2 = Point::new(lex.number()? + ox, lex.number()? + oy);
                let p = Point::new(lex.number()? + ox, lex.number()? + oy);
                flatten_cubic(&mut out, cur, c1, c2, p);
                cur = p;
                last_cubic_ctrl = Some(c2);
                last_quad_ctrl = None;
            }
            'S' => {
                let c1 = match last_cubic_ctrl {
                    Some(prev) => Point::new(2.0 * cur.x - prev.x, 2.0 * cur.y - prev.y),
                    None => cur,
                };
                let c2 = Point::new(lex.number()? + ox, lex.number()? + oy);
                let p = Point::new(lex.number()? + ox, lex.number()? + oy);
                flatten_cubic(&mut out, cur, c1, c2, p);
                cur = p;
                last_cubic_ctrl = Some(c2);
                last_quad_ctrl = None;
            }
            'Z' => {
                // A closed contour's own closing edge is part of its length,
                // exactly as the browser reports it.
                push_pt(&mut out, start);
                cur = start;
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            _ => return None, // an arc, or something not path data at all
        }
    }

    if out.len() < 2 {
        None
    } else {
        Some(out)
    }
}

/// Total length of a polyline, plus the cumulative length at each vertex.
fn cumulative(pts: &[Point]) -> (f64, Vec<f64>) {
    let mut acc = Vec::with_capacity(pts.len());
    let mut total = 0.0;
    acc.push(0.0);
    for i in 1..pts.len() {
        total += (pts[i].x - pts[i - 1].x).hypot(pts[i].y - pts[i - 1].y);
        acc.push(total);
    }
    (total, acc)
}

/// The point at `dist` along a polyline, interpolated within whichever
/// segment covers it — the flattened equivalent of `getPointAtLength`.
fn point_at(pts: &[Point], acc: &[f64], dist: f64) -> Point {
    if dist <= 0.0 {
        return pts[0];
    }
    let last = *acc.last().unwrap_or(&0.0);
    if dist >= last {
        return pts[pts.len() - 1];
    }
    // acc is sorted; find the segment containing `dist`.
    let i = match acc.binary_search_by(|probe| probe.partial_cmp(&dist).unwrap_or(std::cmp::Ordering::Less)) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let i = i.min(pts.len() - 2);
    let seg = acc[i + 1] - acc[i];
    if seg <= 0.0 {
        return pts[i];
    }
    let t = (dist - acc[i]) / seg;
    Point::new(
        pts[i].x + (pts[i + 1].x - pts[i].x) * t,
        pts[i].y + (pts[i + 1].y - pts[i].y) * t,
    )
}

/// Splits path data at each `M`/`m`, keeping the letter's subpaths apart.
fn subpaths(d: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in d.char_indices() {
        if ch == 'M' || ch == 'm' {
            if let Some(s) = start {
                if !d[s..i].trim().is_empty() {
                    out.push(&d[s..i]);
                }
            }
            start = Some(i);
        }
    }
    if let Some(s) = start {
        if !d[s..].trim().is_empty() {
            out.push(&d[s..]);
        }
    }
    out
}

/// One SVG path's subpaths, each resampled to `samples` points spaced evenly
/// by arc length. Subpaths with no length are dropped.
pub fn sample_subpaths(d: &str, samples: usize) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    if samples == 0 {
        return out;
    }
    for sub in subpaths(d) {
        let pts = match flatten_subpath(sub) {
            Some(p) => p,
            None => continue,
        };
        let (total, acc) = cumulative(&pts);
        if !(total > 0.0) {
            continue;
        }
        let mut resampled = Vec::with_capacity(samples);
        for i in 0..samples {
            resampled.push(point_at(&pts, &acc, (i as f64 / samples as f64) * total));
        }
        out.push(resampled);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn a_closed_square_samples_evenly_all_the_way_round() {
        let d = "M0,0 L10,0 L10,10 L0,10 Z";
        let subs = sample_subpaths(d, 40);
        assert_eq!(subs.len(), 1);
        let pts = &subs[0];
        assert_eq!(pts.len(), 40);
        // Perimeter 40, 40 samples: one per unit of arc length, so the tenth
        // sample lands exactly on the first corner.
        assert!(close(pts[10].x, 10.0, 1e-9) && close(pts[10].y, 0.0, 1e-9), "{:?}", pts[10]);
        assert!(close(pts[20].x, 10.0, 1e-9) && close(pts[20].y, 10.0, 1e-9), "{:?}", pts[20]);
        assert!(close(pts[30].x, 0.0, 1e-9) && close(pts[30].y, 10.0, 1e-9), "{:?}", pts[30]);
    }

    #[test]
    fn subpaths_are_kept_apart_rather_than_sampled_across_the_jump() {
        // Two disjoint squares — a letter with a counter, in miniature. A
        // single polyline through both would invent an edge between them.
        let d = "M0,0 L10,0 L10,10 L0,10 ZM30,30 L34,30 L34,34 L30,34 Z";
        let subs = sample_subpaths(d, 16);
        assert_eq!(subs.len(), 2);
        assert!(subs[0].iter().all(|p| p.x <= 10.0 + 1e-9));
        assert!(subs[1].iter().all(|p| p.x >= 30.0 - 1e-9));
    }

    #[test]
    fn a_quadratic_is_flattened_rather_than_ignored() {
        // A half-circle-ish arc: its sampled length must exceed the chord.
        let d = "M0,0 Q10,20 20,0";
        let pts = &sample_subpaths(d, 64)[0];
        let mut len = 0.0;
        for i in 1..pts.len() {
            len += (pts[i].x - pts[i - 1].x).hypot(pts[i].y - pts[i - 1].y);
        }
        assert!(len > 20.0, "a curve is longer than its chord, got {len}");
        assert!(pts.iter().any(|p| p.y > 5.0), "the curve should bulge away from the chord");
    }

    #[test]
    fn a_cubic_is_flattened_too() {
        let d = "M0,0 C0,10 20,10 20,0";
        let pts = &sample_subpaths(d, 32)[0];
        assert!(pts.iter().any(|p| p.y > 3.0));
    }

    #[test]
    fn degenerate_and_unparseable_paths_yield_nothing_rather_than_panicking() {
        assert!(sample_subpaths("", 16).is_empty());
        assert!(sample_subpaths("M5,5", 16).is_empty(), "a lone moveto has no length");
        assert!(sample_subpaths("M0,0 A 5 5 0 0 1 10,10", 16).is_empty(), "arcs are out of scope");
        assert!(sample_subpaths("M0,0 L10,0", 0).is_empty());
    }
}
