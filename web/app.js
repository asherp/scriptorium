// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The playground: a host page wired to the engine.
//
// The split the crate is built around is the thing this page is for looking
// at. The SYMBOLIC derivation — which branches exist, how the grammar unfolded
// — is a pure function of (seed, stage) and is cached forever; drag the column
// narrower or scale the type and the readout's symbol string does not move.
// The TURTLE interpretation — where those branches land, which ones a line of
// text pushed aside — is recomputed from the host's own measured rectangles on
// every pass. Two knob kinds, two clearing rules, and the panel labels which
// is which.

import init, * as engine from './pkg/scriptorium.js';
import { buildParamGroups, buildProductions, buildStages } from './controls.js';
import { measure, obstaclesFrom, renderProse, seedsFrom } from './measure.js';
import { clearOutlineCache } from './trace.js';

const $ = (id) => document.getElementById(id);

const DEFAULT_TEXT = `Beauty sue to tap out a tired roof, and the scribe who ruled these lines
had no thought of the vine that would find them. A margin is not empty; it is
the part of the page that has not yet been asked a question.

Growth answers to the glyph it left, to the text it runs between, to the edge
of the page it may not cross, and to the leash that keeps it belonging to the
mark it grew from. Nothing here is drawn twice the same way by two different
seeds, and nothing here is drawn differently twice by the same one.`;

const state = {
  seed: randomSeed(),
  confirmations: 5000,
  fontSize: 17,
  columnWidth: 520,
  dropCapEm: 3.4,
  anchorMode: 'edge',
  usePage: true,
  rideGlyph: true,
  strokeWidth: 1.1,
  text: DEFAULT_TEXT,
  marked: new Set(['0']),
  show: { leaves: true, obstacles: false, bounds: false, anchors: false, outline: false },
  params: null,
  stages: null,
  termEls: [],
  lastRequest: null,
};

let defaults = null;
let appliedOutlines = '';
let pending = 0;
// Growth is redrawn on every knob drag, but only ANIMATED when what grew has
// actually changed — a new source, a new depth. Replaying it on every frame of
// a slider drag would leave the page permanently half-drawn.
let animateNext = true;

boot();

async function boot() {
  try {
    await init();
  } catch (err) {
    $('boot').innerHTML =
      `<p>The wasm module isn't built yet. Run <code>web/build.sh</code> from the
       repository root, then reload this page.<br><br><code>${escapeHtml(String(err))}</code></p>`;
    return;
  }
  engine.initPanicHook();
  defaults = { params: engine.defaultParams(), stages: engine.defaultGrowthStages() };
  state.params = structuredClone(defaults.params);
  state.stages = structuredClone(defaults.stages);

  wire();
  mountControls();
  layoutText();
  $('boot').classList.add('gone');
  schedule();

  // A font finishing its load changes every measurement on the page and none
  // of the derivation: exactly the case the split exists for.
  document.fonts?.ready.then(() => {
    clearOutlineCache();
    schedule();
  });
  new ResizeObserver(() => schedule()).observe($('column'));
}

/* ── wiring ─────────────────────────────────────────────────────────── */

/**
 * Builds the panel around the params and stages currently in state.
 *
 * A grammar knob shapes the cached symbolic derivation, so changing one has to
 * drop the cache; a turtle knob only moves the walk, which is never cached.
 * That is the whole of the difference, and it is the only thing this callback
 * decides.
 */
function mountControls() {
  buildParamGroups($('params'), state.params, (kind) => {
    if (kind === 'grammar') engine.resetDerivations();
    schedule();
  });
  const grammarChanged = () => {
    engine.resetDerivations();
    schedule();
  };
  buildProductions($('productions'), state.params, grammarChanged);
  buildStages($('stages'), state, grammarChanged);
  buildStageJump();
}

function wire() {
  bindText('seed', 'seed', regrow);
  bindNumber('confirmations', 'confirmations', regrow);
  pairSlider('font-size', 'fontSize', applyPageStyle);
  pairSlider('column-width', 'columnWidth', applyPageStyle);
  pairSlider('dropcap-size', 'dropCapEm', applyPageStyle);
  pairSlider('stroke', 'strokeWidth');

  $('anchor-mode').value = state.anchorMode;
  $('anchor-mode').addEventListener('change', (e) => {
    state.anchorMode = e.target.value;
    schedule();
  });

  bindCheck('use-page', (on) => (state.usePage = on));
  bindCheck('ride-glyph', (on) => (state.rideGlyph = on));
  bindCheck('show-leaves', (on) => (state.show.leaves = on));
  bindCheck('show-obstacles', (on) => (state.show.obstacles = on));
  bindCheck('show-bounds', (on) => (state.show.bounds = on));
  bindCheck('show-anchors', (on) => (state.show.anchors = on));
  bindCheck('show-outline', (on) => (state.show.outline = on));

  $('text').value = state.text;
  $('text').addEventListener('input', () => {
    state.text = $('text').value;
    layoutText();
    schedule();
  });

  $('reroll').addEventListener('click', () => {
    state.seed = randomSeed();
    $('seed').value = state.seed;
    regrow();
  });

  $('reset-params').addEventListener('click', () => {
    state.params = structuredClone(defaults.params);
    state.stages = structuredClone(defaults.stages);
    engine.resetDerivations();
    // The controls hold a reference to the object they mutate, so a fresh
    // params object has to be rebuilt around, not merely re-read.
    mountControls();
    schedule();
  });

  $('replay').addEventListener('click', replay);
  $('download').addEventListener('click', downloadSvg);
  $('copy-request').addEventListener('click', copyRequest);

  $('column').addEventListener('click', (e) => {
    const term = e.target.closest('.term');
    if (!term) return;
    const key = term.dataset.index;
    if (state.marked.has(key)) state.marked.delete(key);
    else state.marked.add(key);
    term.classList.toggle('marked', state.marked.has(key));
    regrow();
  });

  applyPageStyle();
}

function bindText(id, key, after) {
  const el = $(id);
  el.value = state[key];
  el.addEventListener('input', () => {
    state[key] = el.value.trim();
    after?.();
  });
}

function bindNumber(id, key, after) {
  const el = $(id);
  el.value = String(state[key]);
  el.addEventListener('input', () => {
    const n = Number(el.value);
    if (Number.isFinite(n)) {
      state[key] = n;
      after?.();
      schedule();
    }
  });
}

/** A new source or a new depth: what grew changed, so draw it growing. */
function regrow() {
  animateNext = true;
  schedule();
}

function pairSlider(id, key, after) {
  const range = $(id);
  const num = $(`${id}-num`);
  const write = (v) => {
    const n = Number(v);
    if (!Number.isFinite(n)) return;
    state[key] = n;
    range.value = String(n);
    num.value = String(n);
    after?.();
    schedule();
  };
  range.value = String(state[key]);
  num.value = String(state[key]);
  range.addEventListener('input', () => write(range.value));
  num.addEventListener('input', () => write(num.value));
}

function bindCheck(id, set) {
  const el = $(id);
  el.addEventListener('change', () => {
    set(el.checked);
    schedule();
  });
}

function buildStageJump() {
  const root = $('stage-jump');
  root.textContent = '';
  state.stages.forEach((s) => {
    const b = document.createElement('button');
    b.textContent = s.name;
    b.title = `depth ${s.min} — ${Number.isFinite(s.max) ? s.max : '∞'}, ${s.iterations} rewritings`;
    b.addEventListener('click', () => {
      state.confirmations = s.min;
      $('confirmations').value = String(s.min);
      regrow();
    });
    root.append(b);
  });
}

function applyPageStyle() {
  const column = $('column');
  column.style.width = `${state.columnWidth}px`;
  column.style.fontSize = `${state.fontSize}px`;
  document.documentElement.style.setProperty('--dropcap-em', String(state.dropCapEm));
  const cap = column.querySelector('.is-dropcap');
  if (cap) cap.style.fontSize = `${state.dropCapEm}em`;
}

function layoutText() {
  state.termEls = renderProse($('column'), state.text, { dropCap: true });
  for (const el of state.termEls) {
    el.classList.toggle('marked', state.marked.has(el.dataset.index));
  }
  applyPageStyle();
}

/* ── the pass ───────────────────────────────────────────────────────── */

function schedule() {
  if (pending) return;
  pending = requestAnimationFrame(() => {
    pending = 0;
    grow();
  });
}

function grow() {
  const layout = measure($('page'), $('column'), state.termEls);
  const { seeds, marks, outlines } = seedsFrom(layout, state.marked, engine, {
    mode: state.anchorMode,
    rideGlyph: state.rideGlyph,
  });

  // Replacing the table drops the engine's cached sampling with it, so only
  // hand it over when it has actually changed.
  const key = JSON.stringify(outlines);
  if (key !== appliedOutlines) {
    engine.setOutlines(outlines);
    appliedOutlines = key;
  }

  const request = {
    seed: state.seed,
    confirmations: state.confirmations,
    host: layout.host,
    page: state.usePage ? layout.page : null,
    obstacles: obstaclesFrom(layout, state.marked),
    seeds,
    baseSize: state.fontSize,
    params: state.params,
    stages: state.stages,
  };
  state.lastRequest = request;

  const t0 = performance.now();
  let out;
  try {
    out = engine.illuminate(request);
  } catch (err) {
    $('readout').innerHTML = `<dt>error</dt><dd>${escapeHtml(String(err))}</dd>`;
    return;
  }
  const ms = performance.now() - t0;

  draw(out, layout, marks);
  report(out, request, ms);
}

/* ── drawing ────────────────────────────────────────────────────────── */

function draw(out, layout, marks) {
  const svg = $('growth');
  const p = layout.page;
  svg.setAttribute('viewBox', `${r(p.x)} ${r(p.y)} ${r(p.w)} ${r(p.h)}`);
  svg.setAttribute('width', r(p.w));
  svg.setAttribute('height', r(p.h));
  svg.style.left = '0';
  svg.style.top = '0';
  svg.style.width = '100%';
  svg.style.height = '100%';

  const stroke = state.strokeWidth * out.geometryScale;
  const parts = [];

  if (state.show.obstacles) {
    parts.push('<g class="dbg">');
    for (const o of out.obstacles) parts.push(rect(o, 'dbg-obstacle'));
    parts.push('</g>');
  }
  if (state.show.bounds) parts.push(rect(out.bounds, 'dbg-bounds'));
  if (state.show.outline) {
    for (const m of marks) {
      if (!m.outline || !m.inkBox) continue;
      parts.push(
        `<g transform="translate(${r(m.inkBox.x)},${r(m.inkBox.y)}) scale(${r(m.inkBox.w)},${r(m.inkBox.h)})">` +
          `<path class="dbg-outline" vector-effect="non-scaling-stroke" d="${m.outline}"/></g>`
      );
    }
  }

  for (const a of out.anchors) {
    parts.push('<g class="anchor">');
    for (const seg of a.segments) {
      if (seg.d) {
        parts.push(
          `<path class="vine" pathLength="1" stroke-width="${r(stroke)}" d="${seg.d}"/>`
        );
      }
      if (!state.show.leaves) continue;
      for (const leaf of seg.leaves) {
        parts.push(
          `<g class="leaf" transform="${leaf.transform}">` +
            `<path class="blade" d="${leaf.blade}"/>` +
            `<path class="vein" d="${leaf.vein}" stroke-width="${r(stroke * 0.5)}"/>` +
            '</g>'
        );
      }
    }
    if (state.show.anchors) {
      const reach = Math.max(
        state.params.maxReachFloor * out.geometryScale,
        (a.size > 0 ? a.size : 16) * state.params.maxReachMul
      );
      parts.push(`<circle class="dbg-leash" cx="${r(a.x)}" cy="${r(a.y)}" r="${r(reach)}"/>`);
      parts.push(`<circle class="dbg-anchor" cx="${r(a.x)}" cy="${r(a.y)}" r="2.5"/>`);
    }
    parts.push('</g>');
  }

  svg.innerHTML = parts.join('');
  if (animateNext) {
    animateNext = false;
    replay();
  } else {
    // Fresh elements would otherwise inherit a live animation from the class
    // still sitting on the svg, and every static redraw would flicker.
    svg.classList.remove('replay');
  }
}

function rect(b, cls) {
  return `<rect class="${cls}" x="${r(b.x)}" y="${r(b.y)}" width="${r(b.w)}" height="${r(b.h)}"/>`;
}

function r(v) {
  return Math.round(v * 100) / 100;
}

/**
 * Redraws the vines stroke by stroke, each segment a beat behind the last.
 *
 * The stagger is a fraction of a FIXED total rather than a fixed delay per
 * segment: an illuminated initial puts out a couple of thousand of them, and
 * a per-segment beat would leave most of the growth invisible for the best
 * part of a minute.
 */
const REPLAY_SECONDS = 1.4;

function replay() {
  const svg = $('growth');
  svg.classList.remove('replay');
  // Force the animation to restart rather than be coalesced away.
  void svg.getBoundingClientRect();
  const vines = svg.querySelectorAll('.vine');
  const leaves = svg.querySelectorAll('.leaf');
  vines.forEach((el, i) => (el.style.animationDelay = `${((i / Math.max(1, vines.length)) * REPLAY_SECONDS).toFixed(3)}s`));
  leaves.forEach((el, i) => {
    const at = (i / Math.max(1, leaves.length)) * REPLAY_SECONDS;
    el.style.animationDelay = `${(at + 0.25).toFixed(3)}s`;
  });
  svg.classList.add('replay');
}

/* ── readout ────────────────────────────────────────────────────────── */

function report(out, request, ms) {
  const stage = state.stages[out.stage];
  $('stage-chip').textContent = stage ? `${out.stage} · ${stage.name}` : `stage ${out.stage}`;

  const boost = out.anchors[0]?.boost ?? 0;
  const symbol = state.seed
    ? engine.generateSymbol(state.seed, out.stage, boost, state.params, state.stages)
    : '';

  let segments = 0;
  let leaves = 0;
  for (const a of out.anchors) {
    segments += a.segments.length;
    for (const s of a.segments) leaves += s.leaves.length;
  }

  const rows = [
    ['stage', stage ? `${out.stage} (${stage.name})` : String(out.stage)],
    ['rewritings', String((stage?.iterations ?? 0) + boost) + (boost ? ` (${stage?.iterations ?? 0} + ${boost} boost)` : '')],
    ['symbol', `${symbol.length} chars, ${count(symbol, 'F')} F, ${count(symbol, '[')} branches, ${count(symbol, 'L')} L`],
    ['anchors', `${out.anchors.length} of ${request.seeds.length} seed${request.seeds.length === 1 ? '' : 's'}`],
    ['drawn', `${segments} segments, ${leaves} leaves`],
    ['obstacles', String(out.obstacles.length)],
    ['geometry scale', out.geometryScale.toFixed(3)],
    ['illuminate', `${ms.toFixed(2)} ms`],
  ];
  $('readout').innerHTML = rows
    .map(([k, v]) => `<dt>${k}</dt><dd>${escapeHtml(v)}</dd>`)
    .join('');

  const shown = symbol.length > 6000 ? `${symbol.slice(0, 6000)}\n… ${symbol.length - 6000} more` : symbol;
  $('symbol').textContent = shown || '(nothing — a bare stage grows nothing at all)';
}

function count(s, ch) {
  let n = 0;
  for (const c of s) if (c === ch) n++;
  return n;
}

/* ── export ─────────────────────────────────────────────────────────── */

function downloadSvg() {
  const svg = $('growth').cloneNode(true);
  svg.classList.remove('replay');
  svg.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  const cs = getComputedStyle(document.documentElement);
  const style = document.createElementNS('http://www.w3.org/2000/svg', 'style');
  style.textContent = `
    svg { color: ${cs.getPropertyValue('--accent').trim()}; }
    .vine { fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; }
    .blade { fill: currentColor; opacity: .85; }
    .vein { fill: none; stroke: ${cs.getPropertyValue('--paper').trim()}; opacity: .6; }
    .dbg-obstacle, .dbg-bounds, .dbg-leash, .dbg-anchor, .dbg-outline { display: none; }`;
  svg.prepend(style);
  for (const el of svg.querySelectorAll('[style]')) el.removeAttribute('style');

  const blob = new Blob([svg.outerHTML], { type: 'image/svg+xml' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `scriptorium-${state.seed.slice(0, 8)}-stage${$('stage-chip').textContent[0]}.svg`;
  a.click();
  URL.revokeObjectURL(url);
}

async function copyRequest() {
  // f64::INFINITY has no JSON spelling; f64::MAX bucketizes identically and
  // survives the round trip back into the engine.
  const json = JSON.stringify(
    state.lastRequest,
    (_, v) => (typeof v === 'number' && !Number.isFinite(v) ? 1.7976931348623157e308 : v),
    2
  );
  const button = $('copy-request');
  try {
    await navigator.clipboard.writeText(json);
    flash(button, 'copied');
  } catch {
    console.log(json);
    flash(button, 'logged to console');
  }
}

function flash(button, text) {
  const was = button.textContent;
  button.textContent = text;
  setTimeout(() => (button.textContent = was), 1200);
}

/* ── odds and ends ──────────────────────────────────────────────────── */

function randomSeed() {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return [...bytes].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function escapeHtml(s) {
  return String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]);
}
