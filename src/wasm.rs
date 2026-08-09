// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The wasm-bindgen surface: the same engine, addressed from a host page.
//
// Everything crosses as plain JSON-shaped values with camelCase keys, so a
// host hands over the rectangles it measured and gets path data back without
// mirroring any of the logic on its own side. The one piece of state that
// outlives a call — the symbolic derivations, and the sampled glyph outlines —
// lives in a single engine per module instance, exactly as the cache
// contract requires.

use std::cell::RefCell;
use std::collections::HashMap;

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::{
    default_growth_stages, growth_stage as bucket, mark_lead_length as lead_length, outer_contours,
    size_boost as boost_for, GrowthRequest, GrowthStage, Scriptorium, Params, Point,
};

thread_local! {
    static ENGINE: RefCell<Scriptorium> = RefCell::new(Scriptorium::new());
}

/// Plain JS objects and arrays, not `Map`s — a host's control panel binds to
/// `params` directly and clones it, which only a plain object supports.
fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

fn from_js<T: serde::de::DeserializeOwned>(value: JsValue, what: &str) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(|e| JsValue::from_str(&format!("scriptorium: bad {what}: {e}")))
}

/// Routes a Rust panic to the browser console instead of an opaque
/// `unreachable`. Safe to call more than once.
#[wasm_bindgen(js_name = initPanicHook)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// The default tunables. A host's control panel binds to these and hands the
/// mutated object back with every call, so there is exactly one definition of
/// what a knob starts at.
#[wasm_bindgen(js_name = defaultParams)]
pub fn default_params() -> Result<JsValue, JsValue> {
    to_js(&Params::default())
}

/// The default stage ladder.
#[wasm_bindgen(js_name = defaultGrowthStages)]
pub fn default_stages() -> Result<JsValue, JsValue> {
    to_js(&default_growth_stages())
}

/// Registers the host's glyph outlines: `{ "⌘": "M…", … }`, each an SVG path in
/// a unit square. Replaces the whole table, dropping any cached sampling.
#[wasm_bindgen(js_name = setOutlines)]
pub fn set_outlines(outlines: JsValue) -> Result<(), JsValue> {
    let map: HashMap<String, String> = from_js(outlines, "outlines")?;
    ENGINE.with(|e| e.borrow_mut().outlines.set(map));
    Ok(())
}

/// Whether a character has an outline to ride — for a host that wants to know
/// before it goes measuring ink.
#[wasm_bindgen(js_name = hasOutline)]
pub fn has_outline(ch: &str) -> bool {
    ENGINE.with(|e| e.borrow().outlines.contains(ch))
}

/// Clears every cached derivation. Required after any GRAMMAR knob changes, or
/// a seed keeps answering with generations grown under the old rules.
#[wasm_bindgen(js_name = resetDerivations)]
pub fn reset_derivations() {
    ENGINE.with(|e| e.borrow_mut().reset_derivations());
}

/// Which stage a `time` falls in. Pass `null` for the default ladder.
#[wasm_bindgen(js_name = growthStage)]
pub fn growth_stage(time: f64, stages: JsValue) -> Result<usize, JsValue> {
    let stages: Vec<GrowthStage> = if stages.is_undefined() || stages.is_null() {
        default_growth_stages()
    } else {
        from_js(stages, "stages")?
    };
    Ok(bucket(time, &stages))
}

/// The extra generations a mark's own rendered size earns it.
#[wasm_bindgen(js_name = sizeBoost)]
pub fn size_boost(size: f64, base_size: f64, params: JsValue) -> Result<u32, JsValue> {
    let params: Params = if params.is_undefined() || params.is_null() {
        Params::default()
    } else {
        from_js(params, "params")?
    };
    Ok(boost_for(size, base_size, &params))
}

/// The symbol string for one source at one stage — the grammar without the
/// geometry.
#[wasm_bindgen(js_name = generateSymbol)]
pub fn generate_symbol(
    seed: &str,
    stage: usize,
    boost: u32,
    params: JsValue,
    stages: JsValue,
) -> Result<String, JsValue> {
    let params: Params = if params.is_undefined() || params.is_null() {
        Params::default()
    } else {
        from_js(params, "params")?
    };
    let stages: Vec<GrowthStage> = if stages.is_undefined() || stages.is_null() {
        default_growth_stages()
    } else {
        from_js(stages, "stages")?
    };
    Ok(ENGINE.with(|e| e.borrow_mut().generate_symbol(seed, stage, boost, &params, &stages)))
}

/// How much of a seed's text is the mark itself, in UTF-16 code units — the
/// host uses this directly as a DOM text offset. Pass `null` for no override.
#[wasm_bindgen(js_name = markLeadLength)]
pub fn mark_lead_length(text: &str, override_len: Option<f64>) -> usize {
    lead_length(text, override_len)
}

/// The leading whitespace of a text node, in UTF-16 code units.
#[wasm_bindgen(js_name = leadingSpaceLength)]
pub fn leading_space_length(text: &str) -> usize {
    crate::leading_space_len(text)
}

/// Drops every contour some larger contour contains — exposed so a host can
/// check the silhouette rule directly.
#[wasm_bindgen(js_name = outerContours)]
pub fn outer_contours_js(contours: JsValue) -> Result<JsValue, JsValue> {
    let contours: Vec<Vec<Point>> = from_js(contours, "contours")?;
    to_js(&outer_contours(contours))
}

/// Grows one layout's worth of decoration.
#[wasm_bindgen]
pub fn illuminate(request: JsValue) -> Result<JsValue, JsValue> {
    let req: GrowthRequest = from_js(request, "request")?;
    let out = ENGINE.with(|e| e.borrow_mut().illuminate(&req));
    to_js(&out)
}
