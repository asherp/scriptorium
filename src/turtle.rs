// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Turtle interpretation: a symbol string plus a world of rectangles becomes a
// set of polylines.
//
// What the walk answers to, in the order it binds:
//   - THE GLYPH, which a vine leaves by riding its own outer contour (see
//     `Rail`), forking away where following it further would close the letter
//     back onto the trail just drawn.
//   - THE TEXT, as one padded rectangle per term the host laid out. Growth
//     traces those boxes' silhouettes the way a scribe's vine runs between the
//     lines.
//   - THE PAGE, whose own edge bounds growth and, being an edge like any
//     other, gets ridden rather than crashed into.
//   - THE LEASH, so that what grows off a mark still reads as belonging to it
//     and not to some other part of the page.

use std::collections::HashSet;

use crate::geom::{angle_diff, out_of_bounds, segment_hits_rect, tangent_angles, Point, Rect};
use crate::params::Params;
use crate::rng::Rand;

/// The contour a vine rides out of its glyph: the letterform's own silhouette
/// in host space, plus where on it to start and which way round to go.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rail {
    pub pts: Vec<Point>,
    pub start_idx: usize,
    /// `1` or `-1`: which way round the contour to travel.
    pub dir: i32,
    /// The heading the ride sets off on.
    pub tangent: f64,
    /// The contour's own centroid — what "outward" is measured against when
    /// the vine peels off.
    pub cx: f64,
    pub cy: f64,
}

/// A seed point growth springs from.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    /// The initial heading.
    pub angle: f64,
    /// The mark's own rendered size, in px — what earns it extra generations.
    #[serde(default)]
    pub size: f64,
    #[serde(default)]
    pub rail: Option<Rail>,
}

impl Anchor {
    pub fn new(x: f64, y: f64, angle: f64) -> Self {
        Anchor { x, y, angle, size: 0.0, rail: None }
    }
    pub fn with_rail(mut self, rail: Rail) -> Self {
        self.rail = Some(rail);
        self
    }
}

/// A leaf: attached at the vine by its base, pointing along `angle`.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Leaf {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub scale: f64,
}

/// One finished polyline, with whatever leaves were put out along it.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    pub points: Vec<Point>,
    pub leaves: Vec<Leaf>,
}

#[derive(Clone)]
struct State {
    x: f64,
    y: f64,
    angle: f64,
    railing: bool,
    rail_steps: u32,
    /// Obstacles this line of growth is legitimately inside of — see the
    /// long note on `home` in [`interpret_from`].
    home: HashSet<usize>,
}

/// Grows one anchor's decoration.
///
/// `obstacles` and `bounds` are already in the same coordinate space as the
/// anchor. `max_reach` is the leash: no step of any kind may land further than
/// this from where the walk started (pass `f64::INFINITY` for none).
/// `geometry_scale` keeps step and leaf size in proportion to the host's
/// current body text size — angles need no such scaling, only distances do.
///
/// # The home-rect exemption
///
/// Real marks sit flush against the prose they annotate — an opcode mark opens
/// directly against its script's first letter, a citation mark closes directly
/// against the last one, no gap either side. So the anchor point itself is very
/// often already INSIDE the padded rectangle of the very line it marks —
/// sometimes more than one at once: a floated drop cap can make a single text
/// line report as two overlapping client rects, both containing the anchor.
///
/// No amount of steering escapes an obstacle you start inside of — that is a
/// geometric impossibility, not a pathfinding failure — so this carves one
/// exception: EVERY obstacle that contains the anchor at the start (its "home"
/// rects, plural) is ignored while the walk is still inside that specific one,
/// and becomes a normal obstacle again the first time the walk is found
/// outside it — independently per rect, and one-way. An anchor that starts in
/// the clear sees no change at all.
///
/// The exemption belongs to the BRANCH, not the walk: each branch carries its
/// own set, cloned at the fork, so a side shoot that clears the mark cannot
/// prune the exemption out from under a stem still tracing the letter.
// Every one of these is a distinct axis of the walk; bundling them into a
// struct would only move the argument list one line up.
#[allow(clippy::too_many_arguments)]
pub fn interpret_from(
    symbol: &str,
    anchor: &Anchor,
    obstacles: &[Rect],
    bounds: Option<&Rect>,
    rng: &mut dyn Rand,
    max_reach: f64,
    geometry_scale: f64,
    params: &Params,
) -> Vec<Segment> {
    // Read at the top of each call, not captured once — exactly what lets a
    // tuning widget change these between one refresh and the next.
    let step = params.step * geometry_scale;
    let turn = params.turn_deg.to_radians();
    let jitter = params.jitter_deg.to_radians();

    let rail = anchor.rail.as_ref();
    let mut state = State {
        x: anchor.x,
        y: anchor.y,
        angle: anchor.angle,
        railing: rail.is_some(),
        rail_steps: 0,
        home: obstacles
            .iter()
            .enumerate()
            .filter(|(_, r)| r.contains(anchor.x, anchor.y))
            .map(|(i, _)| i)
            .collect(),
    };
    let mut rail_idx = rail.map(|r| r.start_idx).unwrap_or(0);
    let mut stack: Vec<(State, Segment, usize)> = Vec::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut cur = Segment { points: vec![Point::new(state.x, state.y)], leaves: Vec::new() };

    // The obstacles in force for this branch, right now.
    let active = |home: &HashSet<usize>| -> Vec<usize> {
        if home.is_empty() {
            (0..obstacles.len()).collect()
        } else {
            (0..obstacles.len()).filter(|i| !home.contains(i)).collect()
        }
    };
    let first_blocking = |x1: f64, y1: f64, x2: f64, y2: f64, obs: &[usize]| -> Option<usize> {
        obs.iter().copied().find(|&i| segment_hits_rect(x1, y1, x2, y2, &obstacles[i]))
    };
    let within_reach = |x: f64, y: f64| -> bool {
        let (dx, dy) = (x - anchor.x, y - anchor.y);
        dx * dx + dy * dy <= max_reach * max_reach
    };

    for ch in symbol.chars() {
        match ch {
            'F' => {
                let obs = active(&state.home);

                // ── riding the glyph ──────────────────────────────────────
                // While railing, the contour supplies the step outright: no
                // jitter, no turn, just the letter's own line. It ends the
                // moment continuing would double back onto the trail already
                // drawn — a closed contour carries you all the way round to
                // where you began, and the point of this run is the letter's
                // shape, not a loop of it — or when the step budget, the
                // leash, the bounds or a real obstacle says stop.
                if state.railing {
                    if let Some(rail) = rail {
                        let clear = params.glyph_clearance_mul * step;
                        // The trail immediately behind is always "near"; it's
                        // the far end that matters.
                        const SKIP: usize = 3;
                        let adv = if state.rail_steps < params.glyph_follow_max {
                            advance_rail(rail, rail_idx, step)
                        } else {
                            None
                        };
                        let ok = match adv {
                            None => false,
                            Some((_, ax, ay)) => {
                                within_reach(ax, ay)
                                    && !out_of_bounds(ax, ay, bounds)
                                    && first_blocking(state.x, state.y, ax, ay, &obs).is_none()
                                    && !crosses_own_trail(&cur.points, ax, ay, clear, SKIP)
                            }
                        };
                        if ok {
                            let (idx, ax, ay) = adv.expect("checked above");
                            let ra = (ay - state.y).atan2(ax - state.x);
                            rail_idx = idx;
                            state.x = ax;
                            state.y = ay;
                            state.angle = ra;
                            state.rail_steps += 1;
                            cur.points.push(Point::new(ax, ay));
                            // Riding the letter means moving about INSIDE the
                            // box the mark occupies — and a rect the vine is
                            // inside but which is not exempt is a trap it can
                            // never step out of. Anything the ride
                            // legitimately carries the vine inside of joins
                            // the exemption, and is pruned back to a live
                            // obstacle the moment the vine is found outside
                            // it again.
                            for (i, r) in obstacles.iter().enumerate() {
                                if !state.home.contains(&i) && r.contains(ax, ay) {
                                    state.home.insert(i);
                                }
                            }
                            continue;
                        }
                        // Where the vine leaves the letter it turns away from
                        // the contour's own middle, so it peels outward into
                        // the margin rather than back across the glyph it just
                        // traced — and it FORKS: a shoot carries on at twice
                        // that angle, so the ride ends in a branching rather
                        // than in a single quiet turn.
                        let (depart, fork) = depart_from(rail, state.x, state.y, state.angle, params);
                        depart_fork(
                            &mut segments,
                            state.x,
                            state.y,
                            fork,
                            step,
                            params.depart_fork_steps,
                            &obs,
                            obstacles,
                            bounds,
                            &within_reach,
                        );
                        state.angle = depart;
                        state.railing = false;
                    } else {
                        state.railing = false;
                    }
                }

                let wobble = (rng.next_f64() - 0.5) * 2.0 * jitter;
                let a = state.angle + wobble;
                let nx = state.x + a.cos() * step;
                let ny = state.y + a.sin() * step;

                // Leaving `bounds` gets the exact same tangent treatment as
                // meeting an obstacle — "move parallel to the nearest edge" is
                // direction-agnostic. Without this, an anchor near an edge
                // would fall straight to the crude deflection fallback on
                // every single step instead of flowing smoothly around it.
                let hit: Option<Rect> = if out_of_bounds(nx, ny, bounds) || !within_reach(nx, ny) {
                    bounds.copied()
                } else {
                    first_blocking(state.x, state.y, nx, ny, &obs).map(|i| obstacles[i])
                };

                if let Some(hit_rect) = hit {
                    let mut placed = false;
                    // Wall-follow first: steer along whichever of the blocking
                    // box's two tangent directions is closest to where the
                    // vine was already heading. This is what makes growth
                    // trace a paragraph's silhouette — climbing along its edge
                    // and turning its corners — instead of bouncing off it at
                    // a random angle. (When the leash itself is what triggered
                    // this, there is no meaningful edge to trace; the tangent
                    // candidates fail the reach check and fall through to the
                    // deflection fallback below.)
                    let mut tangents = tangent_angles(state.x, state.y, &hit_rect);
                    if angle_diff(a, tangents[1]) < angle_diff(a, tangents[0]) {
                        tangents.swap(0, 1);
                    }
                    for ta in tangents {
                        let tx = state.x + ta.cos() * step;
                        let ty = state.y + ta.sin() * step;
                        if !out_of_bounds(tx, ty, bounds)
                            && within_reach(tx, ty)
                            && first_blocking(state.x, state.y, tx, ty, &obs).is_none()
                        {
                            state.x = tx;
                            state.y = ty;
                            state.angle = ta;
                            cur.points.push(Point::new(tx, ty));
                            placed = true;
                            break;
                        }
                    }
                    // Fallback: the corner case — two obstacles meet, an
                    // obstacle sits flush against the bounds, or the leash
                    // itself is the limit — where no tangent direction is free
                    // either. Widening, increasing deflections either side is
                    // a last resort rather than the first move, and is what
                    // typically turns growth back toward its own anchor once
                    // the leash is what's binding.
                    let mut t = 1;
                    while t <= params.max_deflect_tries && !placed {
                        for sign in [1.0, -1.0] {
                            let da = state.angle + sign * f64::from(t) * (turn / 2.0);
                            let dx = state.x + da.cos() * step;
                            let dy = state.y + da.sin() * step;
                            let blocked = out_of_bounds(dx, dy, bounds)
                                || first_blocking(state.x, state.y, dx, dy, &obs).is_some();
                            if !blocked && within_reach(dx, dy) {
                                state.x = dx;
                                state.y = dy;
                                state.angle = da;
                                cur.points.push(Point::new(dx, dy));
                                placed = true;
                                break;
                            }
                        }
                        t += 1;
                    }
                    if !placed {
                        finish(&mut segments, &mut cur);
                        cur = Segment { points: vec![Point::new(state.x, state.y)], leaves: Vec::new() };
                    }
                } else {
                    state.x = nx;
                    state.y = ny;
                    state.angle = a;
                    cur.points.push(Point::new(nx, ny));
                }

                if !state.home.is_empty() {
                    let (x, y) = (state.x, state.y);
                    state.home.retain(|&i| obstacles[i].contains(x, y));
                }
            }

            'L' => {
                // A heavily size-boosted symbol forks far more branches than a
                // cramped spot has room for; most die on their very first step
                // and land their leaf right back at the SAME unmoved point.
                // Left alone, hundreds of those stack into one dark blob
                // instead of reading as more decoration. Distinct nearby
                // leaves still show (a real cluster); only true near-
                // duplicates collapse.
                let dedup = params.leaf_dedup_px;
                let near = |p: &Leaf| (p.x - state.x).abs() < dedup && (p.y - state.y).abs() < dedup;
                if !cur.leaves.iter().any(near) && !segments.iter().any(|s| s.leaves.iter().any(near)) {
                    // A real vine's leaves don't all point straight along the
                    // stem or come out the same size — each one splays to one
                    // side at its own angle off the vine's heading, and varies
                    // a little in size. Drawn from the same seeded stream as
                    // everything else, so it stays deterministic.
                    let side = if rng.next_f64() < 0.5 { -1.0 } else { 1.0 };
                    let splay = side * (40.0 + rng.next_f64() * 55.0).to_radians();
                    let scale = (0.75 + rng.next_f64() * 0.7) * geometry_scale;
                    cur.leaves.push(Leaf { x: state.x, y: state.y, angle: state.angle + splay, scale });
                }
            }

            '+' => state.angle += turn,
            '-' => state.angle -= turn,

            '[' => {
                // A side branch springs OFF the letter rather than along it:
                // the main stem keeps the rail (restored when this branch
                // closes), the branch itself grows free from the point it
                // left, which is what makes the opening run read as one traced
                // line with shoots coming off it.
                stack.push((state.clone(), std::mem::take(&mut cur), rail_idx));
                cur = Segment { points: vec![Point::new(state.x, state.y)], leaves: Vec::new() };
                state.railing = false;
            }

            ']' => {
                finish(&mut segments, &mut cur);
                if let Some((popped_state, popped_cur, popped_idx)) = stack.pop() {
                    state = popped_state;
                    cur = popped_cur;
                    rail_idx = popped_idx;
                }
            }

            _ => {}
        }
    }
    finish(&mut segments, &mut cur);
    segments
}

fn finish(segments: &mut Vec<Segment>, cur: &mut Segment) {
    if cur.points.len() > 1 || !cur.leaves.is_empty() {
        segments.push(std::mem::take(cur));
    }
}

/// One step's worth of travel along the contour from `idx`, in the direction
/// this anchor settled on — accumulating REAL distance between samples, so a
/// glyph scaled unevenly (the outline is a unit square; the ink box it maps
/// onto rarely is) still advances by a true step.
///
/// Returns `None` when the whole contour has been walked without covering a
/// single step.
fn advance_rail(rail: &Rail, idx: usize, dist: f64) -> Option<(usize, f64, f64)> {
    let n = rail.pts.len();
    if n == 0 || !(dist > 0.0) {
        return None;
    }
    let mut i = idx.min(n - 1);
    let mut acc = 0.0;
    let (mut px, mut py) = (rail.pts[i].x, rail.pts[i].y);
    let mut guard = 0usize;
    while acc < dist {
        guard += 1;
        if guard > n {
            return None;
        }
        i = (i as i64 + rail.dir as i64).rem_euclid(n as i64) as usize;
        acc += (rail.pts[i].x - px).hypot(rail.pts[i].y - py);
        px = rail.pts[i].x;
        py = rail.pts[i].y;
    }
    Some((i, px, py))
}

fn crosses_own_trail(points: &[Point], x: f64, y: f64, clear: f64, skip: usize) -> bool {
    if points.len() <= skip {
        return false;
    }
    points[..points.len() - skip]
        .iter()
        .any(|p| (p.x - x).powi(2) + (p.y - y).powi(2) < clear * clear)
}

/// Where the vine turns when it leaves the letter, and the angle the shoot
/// thrown off at that same point carries on at: the same turn again — twice
/// the bias, the same side — so both leave outward, at a visible angle to
/// each other.
fn depart_from(rail: &Rail, x: f64, y: f64, tangent: f64, params: &Params) -> (f64, f64) {
    let bias = params.glyph_depart_deg.to_radians();
    let left = tangent - std::f64::consts::FRAC_PI_2;
    let away_is_left = left.cos() * (rail.cx - x) + left.sin() * (rail.cy - y) < 0.0;
    let sign = if away_is_left { -1.0 } else { 1.0 };
    (tangent + sign * bias, tangent + sign * bias * 2.0)
}

/// The shoot thrown off where the ride ends: a short straight run, obeying
/// every rule an ordinary step does (leash, bounds, the obstacles live at that
/// moment) and simply stopping at the first one it can't satisfy.
///
/// Deliberately not part of the derivation — it is a property of MEETING the
/// letter's end, which the symbol string knows nothing about — and it carries
/// no leaf, so it reads as the stem splitting rather than as another branch of
/// the grammar.
#[allow(clippy::too_many_arguments)]
fn depart_fork(
    segments: &mut Vec<Segment>,
    x: f64,
    y: f64,
    angle: f64,
    step: f64,
    steps: u32,
    obs: &[usize],
    obstacles: &[Rect],
    bounds: Option<&Rect>,
    within_reach: &dyn Fn(f64, f64) -> bool,
) {
    if steps == 0 {
        return;
    }
    let mut pts = vec![Point::new(x, y)];
    let (mut sx, mut sy) = (x, y);
    for _ in 0..steps {
        let nx = sx + angle.cos() * step;
        let ny = sy + angle.sin() * step;
        let blocked = !within_reach(nx, ny)
            || out_of_bounds(nx, ny, bounds)
            || obs.iter().any(|&i| segment_hits_rect(sx, sy, nx, ny, &obstacles[i]));
        if blocked {
            break;
        }
        sx = nx;
        sy = ny;
        pts.push(Point::new(sx, sy));
    }
    if pts.len() > 1 {
        segments.push(Segment { points: pts, leaves: Vec::new() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Fixed;

    /// A fixed stream with no jitter, so these tests are about the geometry of
    /// the collision response, not about a particular random draw.
    fn flat() -> Fixed {
        Fixed(0.5)
    }

    fn walk(symbol: &str, anchor: &Anchor, obstacles: &[Rect], bounds: Option<&Rect>) -> Vec<Segment> {
        interpret_from(
            symbol,
            anchor,
            obstacles,
            bounds,
            &mut flat(),
            f64::INFINITY,
            1.0,
            &Params::default(),
        )
    }

    fn points(segs: &[Segment]) -> Vec<Point> {
        segs.iter().flat_map(|s| s.points.clone()).collect()
    }

    #[test]
    fn an_unobstructed_trunk_just_walks_straight() {
        let pts = points(&walk(&"F".repeat(10), &Anchor::new(0.0, 0.0, 0.0), &[Rect::new(500.0, 500.0, 50.0, 50.0)], None));
        assert_eq!(pts.len(), 11, "one point per F plus the anchor");
        assert!(pts.iter().all(|p| p.y.abs() < 1e-9));
    }

    #[test]
    fn a_vine_heading_into_a_rectangle_follows_its_edge_rather_than_stopping() {
        let rect = Rect::new(20.0, 0.0, 60.0, 100.0); // spans the anchor's whole path ahead
        let pts = points(&walk(&"F".repeat(40), &Anchor::new(0.0, 50.0, 0.0), &[rect], None));
        assert!(pts.len() > 10, "the trunk should keep advancing along the edge, not die on contact");
        for p in &pts {
            assert!(!rect.contains(p.x, p.y), "point ({},{}) fell inside the rectangle it should trace", p.x, p.y);
        }
    }

    #[test]
    fn wall_following_runs_a_stretch_of_edge_parallel_steps_not_one_deflection() {
        let rect = Rect::new(20.0, 0.0, 60.0, 100.0);
        let pts = points(&walk(&"F".repeat(40), &Anchor::new(0.0, 50.0, 0.0), &[rect], None));
        // Once the trunk meets the left edge it should run several consecutive
        // steps that stay close to that x while y keeps changing in one
        // direction — i.e. it climbs the edge rather than bouncing off it.
        let near_edge: Vec<&Point> = pts.iter().filter(|p| (p.x - rect.x).abs() < 8.0).collect();
        assert!(near_edge.len() >= 6, "expected a real run of edge-hugging points, got {}", near_edge.len());
        let monotonic = near_edge.windows(2).filter(|w| w[1].y > w[0].y).count();
        assert!(
            monotonic + 2 >= near_edge.len(),
            "the edge-hugging run should climb steadily in one direction, not oscillate"
        );
    }

    #[test]
    fn a_leash_keeps_growth_near_its_own_anchor_however_wide_the_bounds() {
        // A viewport-wide `bounds` has no natural corner the way a real
        // obstacle does: once wall-following turns a step to run parallel to
        // it, that direction stops being "blocked" at all, and a long unbroken
        // run in the derivation just keeps going — reading as a separate
        // flourish smeared across the page rather than something that grew
        // from its anchor.
        let anchor = Anchor::new(0.0, 2.0, -std::f64::consts::FRAC_PI_2);
        let bounds = Rect::new(-1000.0, 0.0, 2000.0, 500.0);
        let max_reach = 60.0;
        let segs = interpret_from(
            &"F".repeat(100),
            &anchor,
            &[],
            Some(&bounds),
            &mut flat(),
            max_reach,
            1.0,
            &Params::default(),
        );
        for p in points(&segs) {
            let dist = (p.x - anchor.x).hypot(p.y - anchor.y);
            assert!(dist <= max_reach + 1.0, "point at distance {dist:.1} exceeds the leash ({max_reach})");
        }
    }

    #[test]
    fn with_no_leash_growth_is_unbounded() {
        let anchor = Anchor::new(0.0, 0.0, 0.0);
        let pts = points(&walk(&"F".repeat(20), &anchor, &[], None));
        let last = pts.last().unwrap();
        assert!((last.x - anchor.x).hypot(last.y - anchor.y) > 100.0);
    }

    #[test]
    fn an_anchor_embedded_in_its_own_text_line_still_escapes_and_grows() {
        let home_line = Rect::new(-50.0, -4.0, 400.0, 8.0); // the very line the mark sits on
        let anchor = Anchor::new(0.0, 0.0, -std::f64::consts::FRAC_PI_2); // straight up, out of the line
        assert!(home_line.contains(anchor.x, anchor.y), "the anchor must truly start inside its own line");
        let pts = points(&walk(&"F".repeat(30), &anchor, &[home_line], None));
        assert!(pts.len() > 10, "an embedded anchor must still grow once it clears its own line");
    }

    #[test]
    fn an_anchor_embedded_in_two_overlapping_rects_escapes_both() {
        // A floated drop cap can make one visual line report as TWO
        // overlapping rects — the glyph's own box, and the line box the float
        // sits in. Escaping only one leaves the anchor trapped in the other.
        let glyph_box = Rect::new(-5.0, -10.0, 20.0, 15.0);
        let line_box = Rect::new(-50.0, -10.0, 500.0, 100.0);
        let anchor = Anchor::new(0.0, 0.0, -std::f64::consts::FRAC_PI_2);
        assert!(glyph_box.contains(0.0, 0.0) && line_box.contains(0.0, 0.0));
        let pts = points(&walk(&"F".repeat(30), &anchor, &[glyph_box, line_box], None));
        assert!(pts.len() > 10, "must escape both rects, not stall on the second");
    }

    #[test]
    fn escape_is_one_way_the_same_rect_blocks_again_once_left() {
        // Straight up and out, a clean near-180, then a long run aimed back
        // down at the very line it started on.
        let symbol = format!("{}{}{}", "F".repeat(3), "-".repeat(7), "F".repeat(25));
        let home_line = Rect::new(-50.0, -6.0, 400.0, 16.0);
        let anchor = Anchor::new(0.0, 0.0, -std::f64::consts::FRAC_PI_2);
        let pts = points(&walk(&symbol, &anchor, &[home_line], None));
        // Touching the near edge while wall-following is expected and fine;
        // what "resumes blocking" rules out is coming back out the OTHER side.
        // Skip the anchor itself, which starts deep inside by construction.
        let margin = 4.0;
        let deep = pts[1..]
            .iter()
            .filter(|p| {
                p.x > home_line.x + margin
                    && p.x < home_line.right() - margin
                    && p.y > home_line.y + margin
                    && p.y < home_line.bottom() - margin
            })
            .count();
        assert_eq!(deep, 0, "the home line must resume blocking once the vine has left it");
    }

    #[test]
    fn repeated_leaves_at_the_same_unmoved_point_collapse_rather_than_stacking() {
        // Fifty leaf-only branches, none of which ever moves at all.
        let segs = walk(&"[L]".repeat(50), &Anchor::new(0.0, 0.0, 0.0), &[], None);
        let total: usize = segs.iter().map(|s| s.leaves.len()).sum();
        assert_eq!(total, 1, "fifty leaves at the identical point should collapse to one");
    }

    #[test]
    fn params_step_is_read_live() {
        let params = Params { step: 20.0, ..Default::default() };
        let segs = interpret_from("FF", &Anchor::new(0.0, 0.0, 0.0), &[], None, &mut flat(), f64::INFINITY, 1.0, &params);
        assert_eq!(points(&segs)[1].x, 20.0);
    }

    #[test]
    fn params_turn_deg_is_read_live() {
        let params = Params { turn_deg: 90.0, ..Default::default() };
        let segs = interpret_from("F+F", &Anchor::new(0.0, 0.0, 0.0), &[], None, &mut flat(), f64::INFINITY, 1.0, &params);
        let pts = points(&segs);
        assert!((pts[2].x - pts[1].x).abs() < 1e-9, "a 90 degree turn should move straight in y");
        assert!(pts[2].y > pts[1].y);
    }

    #[test]
    fn geometry_scale_scales_step_distance() {
        let p = Params::default();
        let anchor = Anchor::new(0.0, 0.0, 0.0);
        let unscaled = points(&interpret_from("FF", &anchor, &[], None, &mut flat(), f64::INFINITY, 1.0, &p));
        let doubled = points(&interpret_from("FF", &anchor, &[], None, &mut flat(), f64::INFINITY, 2.0, &p));
        assert_eq!(doubled[1].x, unscaled[1].x * 2.0);
    }

    // ── riding the glyph's own outline ───────────────────────────────────
    // A square contour stands in for a letterform: big enough to ride for
    // several steps, and closed, so following it far enough necessarily
    // returns to the start — which is the case the departure rule exists for.
    fn square_rail(size: f64, per_side: usize) -> Rail {
        let mut pts = Vec::new();
        for i in 0..per_side {
            pts.push(Point::new(size * (i as f64 / per_side as f64), 0.0));
        }
        for i in 0..per_side {
            pts.push(Point::new(size, size * (i as f64 / per_side as f64)));
        }
        for i in 0..per_side {
            pts.push(Point::new(size - size * (i as f64 / per_side as f64), size));
        }
        for i in 0..per_side {
            pts.push(Point::new(0.0, size - size * (i as f64 / per_side as f64)));
        }
        Rail { pts, start_idx: 0, dir: 1, tangent: 0.0, cx: size / 2.0, cy: size / 2.0 }
    }

    #[test]
    fn a_railed_anchor_traces_its_contour_instead_of_striking_out_straight() {
        let rail = square_rail(40.0, 40);
        let anchor = Anchor::new(0.0, 0.0, 0.0).with_rail(rail.clone());
        let pts = points(&walk("FFFFFFFFFF", &anchor, &[], None));
        assert!(pts.iter().any(|p| p.y > 5.0), "growth should have followed the contour round its corner");
        let on_rail = pts[..6]
            .iter()
            .all(|p| rail.pts.iter().any(|q| (q.x - p.x).hypot(q.y - p.y) < 2.0));
        assert!(on_rail, "the opening run should lie on the contour");
    }

    #[test]
    fn the_rail_is_abandoned_before_the_contour_closes_onto_its_own_trail() {
        let rail = square_rail(40.0, 40);
        let anchor = Anchor::new(0.0, 0.0, 0.0).with_rail(rail);
        // Far more F's than a 160px contour has room for at a 7px step, so an
        // unguarded rail would lap it and redraw its own opening.
        let pts = points(&walk(&"F".repeat(120), &anchor, &[], None));
        let returned = pts[12..].iter().filter(|p| p.x.hypot(p.y) < 4.0).count();
        assert_eq!(returned, 0, "the walk lapped the contour and closed back onto its own start");
    }

    #[test]
    fn glyph_follow_max_caps_the_ride_and_zero_disables_railing_outright() {
        let rail = square_rail(40.0, 40);
        let anchor = Anchor::new(0.0, 0.0, 0.0).with_rail(rail.clone());
        let mut params = Params { glyph_follow_max: 0, ..Default::default() };
        let free = points(&interpret_from("FFFFFF", &anchor, &[], None, &mut flat(), f64::INFINITY, 1.0, &params));
        assert!(
            free.iter().all(|p| p.y.abs() < 1e-9 || p.x != 0.0),
            "with glyphFollowMax 0 the walk should not be tracing the contour"
        );

        params.glyph_follow_max = 3;
        let capped = points(&interpret_from(&"F".repeat(40), &anchor, &[], None, &mut flat(), f64::INFINITY, 1.0, &params));
        let on_rail = capped
            .iter()
            .filter(|p| rail.pts.iter().any(|q| (q.x - p.x).hypot(q.y - p.y) < 1.5))
            .count();
        assert!(on_rail <= 6, "expected the railed run capped near 3 steps, got {on_rail} on-contour points");
    }

    #[test]
    fn the_ride_ends_in_a_fork_a_shoot_leaves_with_the_stem_on_the_same_side() {
        let rail = square_rail(40.0, 40);
        let anchor = Anchor::new(0.0, 0.0, 0.0).with_rail(rail);
        // A short ride, so the departure is well inside the F budget.
        let params = Params { glyph_follow_max: 3, ..Default::default() };
        let segs = interpret_from(&"F".repeat(20), &anchor, &[], None, &mut flat(), f64::INFINITY, 1.0, &params);
        assert!(segs.len() >= 2, "departure should leave a shoot beside the stem, got {}", segs.len());
        // Both leave the letter OUTWARD — the shoot is the stem's turn taken
        // twice, not its mirror — so neither ends up back across the contour's
        // own middle (the square's centre is (20,20)).
        for s in &segs {
            let t = s.points.last().unwrap();
            assert!(
                !(t.x > 2.0 && t.x < 38.0 && t.y > 2.0 && t.y < 38.0),
                "a departing branch ended at ({:.1},{:.1}), back inside the letter it left",
                t.x,
                t.y
            );
        }
    }

    #[test]
    fn depart_fork_steps_zero_leaves_the_plain_departure() {
        let rail = square_rail(40.0, 40);
        let anchor = Anchor::new(0.0, 0.0, 0.0).with_rail(rail);
        let params = Params { glyph_follow_max: 3, depart_fork_steps: 0, ..Default::default() };
        let segs = interpret_from(&"F".repeat(20), &anchor, &[], None, &mut flat(), f64::INFINITY, 1.0, &params);
        assert_eq!(segs.len(), 1, "with the fork off, one unbranched run should leave the letter");
    }

    #[test]
    fn an_anchor_with_no_rail_walks_exactly_as_it_would_have() {
        let pts = points(&walk("FFFFF", &Anchor::new(0.0, 0.0, 0.0), &[], None));
        assert!(pts.iter().all(|p| p.y.abs() < 1e-9), "an unrailed straight run should stay straight");
    }
}
