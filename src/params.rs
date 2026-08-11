// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Every knob the L-system and its turtle answer to, gathered in one place so
// a host (a tuning widget, in particular) can mutate them and re-render.
//
// Nothing here is read into a local and forgotten: every consumer takes
// `&Params` and reads the field at the moment it is used, so a change takes
// effect on the very next symbol or layout it touches.
//
// Two kinds of knob, and they clear differently:
//   - GRAMMAR knobs (`productions`, the branchiness ramp, `max_size_boost`)
//     shape the SYMBOLIC derivation, which is cached forever per source —
//     change one and the cache must be reset (see `Scriptorium::reset_derivations`).
//   - TURTLE knobs (everything else) only affect the geometric walk, which is
//     never cached; a plain re-layout is enough.

use serde::{Deserialize, Serialize};

/// One rewriting rule. `weight` is its share of the draw; a `reserved` rule is
/// discarded outright until branchiness clears
/// [`Params::branchiness_fork_threshold`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Production {
    pub weight: f64,
    pub to: String,
    #[serde(default)]
    pub reserved: bool,
}

impl Production {
    fn new(weight: f64, to: &str, reserved: bool) -> Self {
        Production { weight, to: to.to_string(), reserved }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Params {
    // ── grammar ───────────────────────────────────────────────────────────
    pub productions: Vec<Production>,
    /// How fast branchiness ramps up per generation.
    pub branchiness_per_gen: f64,
    /// The ramp's ceiling.
    pub branchiness_cap: f64,
    /// Branchiness needed before `reserved` productions can be picked.
    pub branchiness_fork_threshold: f64,
    /// [`crate::size_boost`]'s own ceiling, in generations.
    pub max_size_boost: u32,

    // ── turtle ────────────────────────────────────────────────────────────
    /// Pixels per `F`.
    pub step: f64,
    /// Base turn angle, in degrees (`+` / `-`).
    pub turn_deg: f64,
    /// Random wobble added to every `F`'s heading, in degrees.
    pub jitter_deg: f64,
    /// How hard a blocked step tries to dodge before giving up.
    pub max_deflect_tries: u32,
    /// Leaves within this many px of one already placed collapse.
    pub leaf_dedup_px: f64,
    /// Halo around each obstacle rectangle, in px.
    pub obstacle_pad: f64,
    /// How far past the host's own box a vine may roam, WITHOUT a page to
    /// bound it.
    pub overflow: f64,
    /// How far inside the page's own edge growth stops, given one.
    pub bounds_inset: f64,
    /// The leash's minimum radius from an anchor.
    ///
    /// A mark riding a block's silhouette overrides this upward to that
    /// silhouette's own reach: a border belongs to the paragraph it wraps, so
    /// stopping it short of one would read as a failure rather than as
    /// restraint.
    pub max_reach_floor: f64,
    /// The leash's radius per px of the anchor's own size.
    pub max_reach_mul: f64,
    /// How far OUTSIDE the obstacle halo a block's silhouette is ruled, in px.
    ///
    /// A border ruled on the writing itself is a border no vine can ride: it
    /// would be blocked by the very text it wraps on its first step. The ring
    /// is the letters' own hull grown by [`Params::obstacle_pad`] plus this
    /// (see [`crate::block_hull`]).
    ///
    /// The default clears a body line's LEADING as well as the halo, which is
    /// what it has to do wherever a host's obstacle field is per term rather
    /// than per letter: a line's box stands several pixels above its tallest
    /// ascender and below its deepest descender, and a ring that only cleared
    /// the ink would be inside the box.
    pub hull_margin: f64,

    // ── riding a rail: a glyph's outline, or a block's silhouette ─────────
    /// A CAP on how many steps the opening run may spend tracing its rail — a
    /// closed ring would otherwise carry it all the way round and back onto
    /// its own trail.
    ///
    /// A cap and not a length. How far a vine actually rides is decided by the
    /// GRAMMAR: a branch springs off the rail rather than along it, so the
    /// ride is exactly as long as the run of `F`s outside any `[`, and every
    /// `F` inside a bracket is growth that leaves. Raising this does nothing
    /// at all until that trunk is longer than it — which, with the default
    /// productions, it is not until well past the deepest stage.
    ///
    /// So a host that wants a vine to run a whole paragraph reaches for
    /// [`Params::productions`], not for this: a rule that keeps two `F`s in
    /// the trunk lays down twice the border of one that keeps one.
    pub glyph_follow_max: u32,
    /// How near (in step-lengths) its own earlier trail has to come before
    /// the ride counts as about to cross itself.
    pub glyph_clearance_mul: f64,
    /// How sharply the vine turns away from its rail when it leaves.
    pub glyph_depart_deg: f64,
    /// Where it leaves, it FORKS: a shoot this many steps long carries on at
    /// twice the departure angle. 0 disables it, leaving a plain departure.
    pub depart_fork_steps: u32,

    /// The body-text size step and leaf drawing were tuned at. A host scales
    /// distances by (its current body size / this) so the decoration stays in
    /// proportion to the letters rather than at a fixed pixel size.
    pub reference_font_size: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            productions: vec![
                Production::new(3.0, "F", false),               // plain growth
                Production::new(3.0, "FL", false),              // growth capped with a leaf
                Production::new(2.0, "F[+F]F", false),          // a side branch, trunk continues
                Production::new(2.0, "F[-FL]F", false),
                Production::new(1.0, "F[+FL][-FL]F", true),     // a fuller fork, deep stages only
            ],
            branchiness_per_gen: 0.14,
            branchiness_cap: 0.8,
            branchiness_fork_threshold: 0.6,
            max_size_boost: 6,

            step: 7.0,
            turn_deg: 24.0,
            jitter_deg: 10.0,
            max_deflect_tries: 6,
            leaf_dedup_px: 3.0,
            obstacle_pad: 2.5,
            overflow: 48.0,
            bounds_inset: 2.0,
            max_reach_floor: 80.0,
            max_reach_mul: 1.8,
            hull_margin: 10.0,

            glyph_follow_max: 26,
            glyph_clearance_mul: 0.9,
            glyph_depart_deg: 52.0,
            depart_fork_steps: 3,

            reference_font_size: 16.0,
        }
    }
}

/// Age, bucketed: a confirmation-style depth count maps to a discrete stage.
///
/// Buckets, not a continuous function of depth. A stage is a cache key; a
/// smooth mapping would mean nothing is ever settled enough to cache or to
/// stop re-animating.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrowthStage {
    pub min: f64,
    /// `f64::INFINITY` for the saturating last bucket.
    pub max: f64,
    pub name: String,
    pub iterations: u32,
}

impl GrowthStage {
    fn new(min: f64, max: f64, name: &str, iterations: u32) -> Self {
        GrowthStage { min, max, name: name.to_string(), iterations }
    }
}

/// The default ladder: bare, curl, vine, bordered, illuminated.
pub fn default_growth_stages() -> Vec<GrowthStage> {
    vec![
        GrowthStage::new(0.0, 0.0, "bare", 0),
        GrowthStage::new(1.0, 5.0, "curl", 1),
        GrowthStage::new(6.0, 143.0, "vine", 3),
        GrowthStage::new(144.0, 4031.0, "bordered", 4),
        GrowthStage::new(4032.0, f64::INFINITY, "illuminated", 6),
    ]
}

/// Which bucket a depth count falls in. Negative and non-finite counts read as
/// zero; a count past the last bucket saturates there rather than falling out.
pub fn growth_stage(confirmations: f64, stages: &[GrowthStage]) -> usize {
    if stages.is_empty() {
        return 0;
    }
    let n = if confirmations.is_finite() { confirmations.max(0.0) } else { 0.0 };
    stages
        .iter()
        .position(|s| n >= s.min && n <= s.max)
        .unwrap_or(stages.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growth_stage_buckets_the_count_rather_than_scaling_with_it() {
        let s = default_growth_stages();
        assert_eq!(growth_stage(0.0, &s), 0, "unconfirmed is stage 0 (bare)");
        assert_eq!(growth_stage(1.0, &s), 1);
        assert_eq!(growth_stage(5.0, &s), 1);
        assert_eq!(growth_stage(6.0, &s), 2);
        assert_eq!(growth_stage(143.0, &s), 2);
        assert_eq!(growth_stage(144.0, &s), 3);
        assert_eq!(growth_stage(4031.0, &s), 3);
        assert_eq!(growth_stage(4032.0, &s), 4);
        assert_eq!(growth_stage(1_000_000.0, &s), 4, "stage saturates rather than growing without bound");
    }

    #[test]
    fn growth_stage_reads_a_nonsense_count_as_zero() {
        let s = default_growth_stages();
        assert_eq!(growth_stage(-17.0, &s), 0);
        assert_eq!(growth_stage(f64::NAN, &s), 0);
    }

    #[test]
    fn stage_edges_are_contiguous_so_no_count_falls_through() {
        let s = default_growth_stages();
        for i in 1..s.len() {
            assert_eq!(s[i].min, s[i - 1].max + 1.0, "gap between stage {} and stage {i}", i - 1);
        }
    }
}
