// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The grammar: a stochastic vine/flower L-system (Prusinkiewicz-style).
//
// Alphabet: `F` = grow one step and draw it, `L` = put out a leaf here (no
// movement), `+` / `-` = turn, `[` / `]` = push/pop a branch point. The rule
// table itself lives in `Params::productions`.
//
// One continuous derivation per source, not an independent roll per stage.
// This is the point, not just an optimization: every production keeps at least
// one `F`, so a later generation can only elaborate an earlier one, never come
// out sparser — stage N's shape is always stage (N-1)'s shape grown one step
// further, the way real growth accumulates rather than getting re-decided from
// scratch every time a reader checks back on something older.

use std::collections::HashMap;

use crate::params::{GrowthStage, Params, Production};
use crate::rng::{Rand, Rng};

/// Bias toward the branchier rules as `branchiness` (pass-driven) rises, by
/// discarding `reserved` options outright below a threshold — simpler than
/// re-weighting the whole table per pass, and easy to reason about.
fn pick_production<'a>(rng: &mut dyn Rand, branchiness: f64, params: &'a Params) -> &'a str {
    let reserved_allowed = branchiness > params.branchiness_fork_threshold;
    let pool: Vec<&Production> = params
        .productions
        .iter()
        .filter(|p| reserved_allowed || !p.reserved)
        .collect();
    let Some(&last) = pool.last() else {
        // A table with nothing drawable left still has to keep the axiom
        // alive; growing nothing would erase the vine rather than simplify it.
        return "F";
    };
    let total: f64 = pool.iter().map(|p| p.weight).sum();
    let mut r = rng.next_f64() * total;
    for p in pool.iter().copied() {
        r -= p.weight;
        if r <= 0.0 {
            return &p.to;
        }
    }
    &last.to
}

/// One source's derivation: the stream it rolls from, and every generation
/// produced so far. `passes[i]` is the symbol string after `i` rewritings.
struct Derivation {
    rng: Rng,
    passes: Vec<String>,
}

/// The symbolic half of the engine: grammar state that must NOT change across
/// a reflow, a resize, or an orientation flip.
#[derive(Default)]
pub struct Derivations {
    cache: HashMap<String, Derivation>,
}

impl Derivations {
    pub fn new() -> Self {
        Derivations { cache: HashMap::new() }
    }

    /// Clears every cached derivation.
    ///
    /// Grammar knobs (`params.productions`, the branchiness ramp,
    /// `max_size_boost`) all shape what is cached here. Call this after
    /// changing any of them — a tuning widget does, on every control's input
    /// event — or a source would keep answering with generations grown under
    /// the OLD rules forever.
    pub fn reset(&mut self) {
        self.cache.clear();
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// The symbol string for `(seed, stage)` after that stage's iteration
    /// count plus any size-driven `boost`, derived and cached incrementally
    /// from generation 0 up.
    ///
    /// `boost` extends the SAME continuous derivation further rather than
    /// rolling a separate one, so a large mark's vine is this source's shape
    /// grown a few generations past where a small mark's stops, never a
    /// different shape entirely.
    pub fn generate_symbol(
        &mut self,
        seed: &str,
        stage: usize,
        boost: u32,
        params: &Params,
        stages: &[GrowthStage],
    ) -> String {
        let spec_iterations = stages
            .get(stage)
            .or_else(|| stages.first())
            .map(|s| s.iterations)
            .unwrap_or(0);
        let max_generation = stages.last().map(|s| s.iterations).unwrap_or(0) + params.max_size_boost;
        let target = max_generation.min(spec_iterations + boost) as usize;

        let rec = self
            .cache
            .entry(seed.to_string())
            .or_insert_with(|| Derivation { rng: Rng::from_hex(seed), passes: vec!["F".to_string()] });

        while rec.passes.len() - 1 < target {
            let pass_index = rec.passes.len(); // the generation about to be produced
            let branchiness = params
                .branchiness_cap
                .min(pass_index as f64 * params.branchiness_per_gen);
            let prev = rec.passes[rec.passes.len() - 1].clone();
            let mut next = String::with_capacity(prev.len() * 2);
            for ch in prev.chars() {
                if ch == 'F' {
                    next.push_str(pick_production(&mut rec.rng, branchiness, params));
                } else {
                    next.push(ch);
                }
            }
            rec.passes.push(next);
        }
        rec.passes[target].clone()
    }
}

/// Extra rewriting generations a mark earns for its own rendered size.
///
/// A page's illuminated initial (a drop cap at 3.4em, say) is meant to outgrow
/// an inline glyph at body size, the way a real manuscript's decoration
/// answers to the letter it grows from as much as to the page's age.
/// `base_size` is the "ordinary mark" reference (a body-text line height);
/// ratios at or below that earn no boost at all — most marks on a page are
/// exactly that ordinary, and shouldn't all be growing extra generations by
/// default.
///
/// log2-scaled, so a 2x mark and a 4x mark are visibly different without a
/// merely-larger mark blowing past the cap.
pub fn size_boost(size: f64, base_size: f64, params: &Params) -> u32 {
    if !(size > 0.0) || !(base_size > 0.0) || size <= base_size {
        return 0;
    }
    let raw = ((size / base_size).log2() * 2.0).round();
    if raw <= 0.0 {
        return 0;
    }
    (raw as u32).min(params.max_size_boost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::default_growth_stages;

    #[test]
    fn a_symbol_is_deterministic_for_the_same_seed_and_stage() {
        let params = Params::default();
        let stages = default_growth_stages();
        let hash = "00000000000000000009a5b2b9c4de6c9c1c9b3e9e9a5b2b9c4de6c9c1c9b3e";
        let mut d = Derivations::new();
        let a = d.generate_symbol(hash, 3, 0, &params, &stages);
        let b = d.generate_symbol(hash, 3, 0, &params, &stages);
        assert_eq!(a, b, "the same source at the same stage must read the same to every viewer");

        // …and to a cache that has never seen it before.
        let mut fresh = Derivations::new();
        assert_eq!(fresh.generate_symbol(hash, 3, 0, &params, &stages), a);
    }

    #[test]
    fn deeper_stages_of_one_source_do_not_all_collapse_together() {
        let params = Params::default();
        let stages = default_growth_stages();
        let mut d = Derivations::new();
        let grown: Vec<String> = (0..stages.len())
            .map(|i| d.generate_symbol("cafebabe".repeat(8).as_str(), i, 0, &params, &stages))
            .collect();
        assert_eq!(grown[0], "F", "stage 0 has no iterations — the bare axiom");
        assert!(grown.iter().collect::<std::collections::HashSet<_>>().len() > 1);
    }

    #[test]
    fn two_different_sources_do_not_illuminate_identically() {
        let params = Params::default();
        let stages = default_growth_stages();
        let mut d = Derivations::new();
        let a = d.generate_symbol(&"1".repeat(64), 3, 0, &params, &stages);
        let b = d.generate_symbol(&"2".repeat(64), 3, 0, &params, &stages);
        assert_ne!(a, b);
    }

    #[test]
    fn growth_only_ever_rewrites_f_and_stays_in_the_alphabet() {
        let params = Params::default();
        let stages = default_growth_stages();
        let mut d = Derivations::new();
        let sym = d.generate_symbol(&"deadbeef".repeat(8), 4, 0, &params, &stages);
        assert!(!sym.is_empty());
        assert!(
            sym.chars().all(|c| matches!(c, 'F' | 'L' | '+' | '-' | '[' | ']')),
            "symbol string must stay within the L-system alphabet: {sym}"
        );
    }

    #[test]
    fn a_later_generation_only_ever_elaborates_an_earlier_one() {
        let params = Params::default();
        let stages = default_growth_stages();
        let mut d = Derivations::new();
        let mut prev = 0usize;
        for stage in 0..stages.len() {
            let n = d
                .generate_symbol("accumulating-growth", stage, 0, &params, &stages)
                .matches('F')
                .count();
            assert!(n >= prev, "stage {stage} came out sparser than the one before it");
            prev = n;
        }
    }

    #[test]
    fn boost_extends_the_same_derivation_is_capped_and_caches_correctly() {
        let params = Params::default();
        let stages = default_growth_stages();
        let mut d = Derivations::new();
        let hash = "illuminated-initial-test";
        let base = d.generate_symbol(hash, 2, 0, &params, &stages);
        let boosted = d.generate_symbol(hash, 2, 4, &params, &stages);
        assert_ne!(base, boosted, "a boosted anchor should read further into the derivation");
        // Asking again for the unboosted generation must return exactly what
        // it did before: computing a later generation must not retroactively
        // change an earlier one that has already been read.
        assert_eq!(d.generate_symbol(hash, 2, 0, &params, &stages), base);
        // An outlandish boost is clamped rather than growing without bound.
        let capped = d.generate_symbol(hash, 4, 1000, &params, &stages);
        assert_eq!(d.generate_symbol(hash, 4, 1000, &params, &stages), capped);
        assert_eq!(capped, d.generate_symbol(hash, 4, params.max_size_boost, &params, &stages));
    }

    #[test]
    fn reset_is_required_for_a_grammar_change_to_reach_an_already_seen_source() {
        let stages = default_growth_stages();
        let mut params = Params::default();
        let mut d = Derivations::new();
        let hash = "params-live-tuning-test";
        let before = d.generate_symbol(hash, 4, 0, &params, &stages);

        // Force every production to the richest, always-branching rule — a
        // maximally different grammar from the default table.
        params.productions = vec![crate::params::Production {
            weight: 1.0,
            to: "F[+F][-F]F".to_string(),
            reserved: false,
        }];
        assert_eq!(
            d.generate_symbol(hash, 4, 0, &params, &stages),
            before,
            "a grammar change must not silently alter an already-cached derivation"
        );
        d.reset();
        assert_ne!(
            d.generate_symbol(hash, 4, 0, &params, &stages),
            before,
            "after reset(), the SAME source should re-derive under the NEW grammar"
        );
    }

    #[test]
    fn reserved_productions_stay_out_until_branchiness_clears_the_threshold() {
        let params = Params {
            productions: vec![
                Production { weight: 1.0, to: "F".to_string(), reserved: false },
                Production { weight: 1000.0, to: "F[+FL][-FL]F".to_string(), reserved: true },
            ],
            ..Default::default()
        };
        let stages = default_growth_stages();
        let mut d = Derivations::new();
        // Stage 1 is a single generation: branchiness is 0.14, well under the
        // 0.6 threshold, so the heavily-weighted reserved rule cannot be drawn.
        assert_eq!(d.generate_symbol("threshold", 1, 0, &params, &stages), "F");
    }

    #[test]
    fn size_boost_is_zero_at_or_below_the_baseline() {
        let p = Params::default();
        assert_eq!(size_boost(16.0, 16.0, &p), 0);
        assert_eq!(size_boost(10.0, 16.0, &p), 0);
        assert_eq!(size_boost(0.0, 16.0, &p), 0);
        assert_eq!(size_boost(f64::NAN, 16.0, &p), 0);
    }

    #[test]
    fn size_boost_grows_with_size_and_saturates() {
        let p = Params::default();
        let small = size_boost(16.0, 16.0, &p);
        let medium = size_boost(32.0, 16.0, &p); // a 2x mark
        let large = size_boost(54.0, 16.0, &p); // ~3.4em against a 16px body — a drop cap
        let huge = size_boost(100_000.0, 16.0, &p);
        assert!(small < medium, "a 2x mark should out-boost the baseline");
        assert!(medium < large, "the drop-cap ratio should out-boost a merely-larger mark");
        assert!(large <= huge && huge <= p.max_size_boost);
    }
}
