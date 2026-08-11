// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The playground: a host page wired to the engine.
//
// The split the crate is built around is the thing this page is for looking
// at. The SYMBOLIC derivation — which branches exist, how the grammar unfolded
// — is a pure function of (seed, stage) and is cached forever; drag the column
// narrower or scale the type and the readout's symbol string does not move.
// The TURTLE interpretation — where those branches land, which ones a line of
// text pushed aside, which silhouette a border ran — is recomputed from the
// host's own measured rectangles on every pass. Two knob kinds, two clearing
// rules, and the panel labels which is which.

import init, * as engine from './pkg/scriptorium.js';
import { buildNotation, buildParamGroups, buildProductions, buildStages } from './controls.js';
import { blocksFrom, markedTerms, measure, obstaclesFrom, renderProse, seedsFrom } from './measure.js';
import { DEFAULT_TEXT, NOTATION, marksByLength } from './notation.js';
import { clearOutlineCache } from './trace.js';

const $ = (id) => document.getElementById(id);
const ALL_MARKS = marksByLength();

const state = {
  seed: randomSeed(),
  // A page whose every opcode is a mark grows a great many vines at once, so
  // it opens partway up the ladder rather than at the top of it: the marks
  // read as marks, and the depth is one drag from illuminated.
  confirmations: 6,
  fontSize: 17,
  columnWidth: 520,
  dropCapEm: 3.4,
  siglaEm: 1,
  anchorMode: 'edge',
  usePage: true,
  rideGlyph: true,
  rideBlock: true,
  dropCapIsMark: true,
  strokeWidth: 1.1,
  text: DEFAULT_TEXT,
  /// Opcodes switched OFF by name. Empty means the whole notation is live.
  disabledOps: new Set(),
  /// Terms the reader has overridden by clicking, either way.
  manual: new Map(),
  show: { leaves: true, obstacles: false, bounds: false, anchors: false, outline: false, hulls: true },
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
  buildNotation($('notation'), NOTATION, state, schedule);
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
  pairSlider('sigla-size', 'siglaEm', applyPageStyle);
  pairSlider('stroke', 'strokeWidth');

  $('anchor-mode').value = state.anchorMode;
  $('anchor-mode').addEventListener('change', (e) => {
    state.anchorMode = e.target.value;
    schedule();
  });

  bindCheck('use-page', (on) => (state.usePage = on));
  bindCheck('ride-block', (on) => (state.rideBlock = on), regrow);
  bindCheck('ride-glyph', (on) => (state.rideGlyph = on));
  bindCheck('dropcap-mark', (on) => (state.dropCapIsMark = on));
  bindCheck('show-leaves', (on) => (state.show.leaves = on));
  bindCheck('show-obstacles', (on) => (state.show.obstacles = on));
  bindCheck('show-bounds', (on) => (state.show.bounds = on));
  bindCheck('show-anchors', (on) => (state.show.anchors = on));
  bindCheck('show-outline', (on) => (state.show.outline = on));
  bindCheck('show-hulls', (on) => (state.show.hulls = on));

  $('text').value = state.text;
  $('text').addEventListener('input', () => {
    state.text = $('text').value;
    state.manual.clear(); // term indices no longer mean what they meant
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
    state.manual.set(key, !term.classList.contains('marked'));
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

/** A new source, a new depth, a new mark: what grew changed, so draw it growing. */
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

function bindCheck(id, set, after) {
  const el = $(id);
  el.addEventListener('change', () => {
    set(el.checked);
    (after ?? schedule)();
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
  // The sigla answer to their own scale, not to the body's. A notation is set
  // at the size its marks need to be legible at, which is rarely the size of
  // the prose around it — and a mark's rendered size is also what earns it
  // extra generations, so this is a growth control as much as a typographic
  // one.
  column.style.setProperty('--sigla', String(state.siglaEm));
  const cap = column.querySelector('.is-dropcap');
  if (cap) cap.style.fontSize = `${state.dropCapEm}em`;
}

function layoutText() {
  state.termEls = renderProse($('column'), state.text, { dropCap: true });
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
  // Which terms are marks: the notation decides, a click overrides. This has
  // to happen BEFORE the page is measured — a mark carries its own size, so
  // marking one changes the box it will be measured at.
  const marks = ALL_MARKS.filter((m) => !state.disabledOps.has(m.name));
  const marked = markedTerms(state.termEls, marks, {
    dropCap: state.dropCapIsMark,
    manual: state.manual,
  });
  for (const el of state.termEls) el.classList.toggle('marked', marked.has(el.dataset.index));

  const layout = measure($('page'), $('column'), state.termEls);

  // The blocks, as characters and where each one puts ink — which is what the
  // silhouette is a hull of. The outlines come with them: without the letters'
  // own contours the ring would wrap the boxes they sit in instead.
  const { blocks, outlines } = blocksFrom(layout, { rideGlyph: state.rideGlyph });

  // Replacing the table drops the engine's cached sampling with it, so only
  // hand it over when it has actually changed.
  const key = JSON.stringify(outlines);
  if (key !== appliedOutlines) {
    engine.setOutlines(outlines);
    appliedOutlines = key;
  }

  const { seeds, details } = seedsFrom(layout, marked, engine, {
    mode: state.anchorMode,
    rideBlock: state.rideBlock,
  });

  const request = {
    seed: state.seed,
    confirmations: state.confirmations,
    host: layout.host,
    page: state.usePage ? layout.page : null,
    obstacles: obstaclesFrom(layout, marked),
    blocks,
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

  // The last exchange, for a console. Everything the engine was told and
  // everything it answered, in the coordinate space the page is drawn in —
  // which is the difference between guessing why a vine went somewhere and
  // reading off where it was allowed to go.
  window.scriptorium = { request, response: out, blocks, outlines, layout };

  draw(out, layout, blocks, outlines);
  report(out, request, details, ms);
}

/* ── drawing ────────────────────────────────────────────────────────── */

function draw(out, layout, blocks, outlines) {
  const svg = $('growth');
  const p = layout.page;
  svg.setAttribute('viewBox', `${r(p.x)} ${r(p.y)} ${r(p.w)} ${r(p.h)}`);
  svg.setAttribute('width', r(p.w));
  svg.setAttribute('height', r(p.h));
  svg.style.inset = '0';
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
  if (state.show.hulls) {
    // Two rings, and the gap between them is the point. The tight one is the
    // letters' own convex hull, its vertices sitting ON the writing. The other
    // is that hull grown clear of the text's halo, which is the one growth
    // actually rides — a ring drawn on the letters would be blocked by them.
    for (const glyphs of blocks) {
      const tight = glyphs.length ? engine.blockHull(glyphs, 0) : [];
      if (tight.length < 3) continue;
      parts.push(`<path class="dbg-hull-tight" d="${ring(tight)}"/>`);
    }
    for (const hull of out.hulls) {
      if (hull.length < 3) continue;
      parts.push(`<path class="dbg-hull" d="${ring(hull)}"/>`);
    }
  }
  if (state.show.outline) {
    for (const block of blocks) {
      for (const g of block) {
        const d = outlines[g.ch];
        if (!d || !(g.box.w > 0) || !(g.box.h > 0)) continue;
        parts.push(
          `<g transform="translate(${r(g.box.x)},${r(g.box.y)}) scale(${r(g.box.w)},${r(g.box.h)})">` +
            `<path class="dbg-outline" vector-effect="non-scaling-stroke" d="${d}"/></g>`
        );
      }
    }
  }

  for (const a of out.anchors) {
    parts.push('<g class="anchor">');
    for (const seg of a.segments) {
      if (seg.d) {
        parts.push(`<path class="vine" pathLength="1" stroke-width="${r(stroke)}" d="${seg.d}"/>`);
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

function ring(points) {
  return `${points.map((q, i) => `${i ? 'L' : 'M'}${r(q.x)},${r(q.y)}`).join(' ')} Z`;
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
  vines.forEach((el, i) => {
    el.style.animationDelay = `${((i / Math.max(1, vines.length)) * REPLAY_SECONDS).toFixed(3)}s`;
  });
  leaves.forEach((el, i) => {
    const at = (i / Math.max(1, leaves.length)) * REPLAY_SECONDS;
    el.style.animationDelay = `${(at + 0.25).toFixed(3)}s`;
  });
  svg.classList.add('replay');
}

/* ── readout ────────────────────────────────────────────────────────── */

function report(out, request, details, ms) {
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
  const bordered = request.seeds.filter((s) => s.block !== null && s.block !== undefined).length;
  const grew = out.anchors.filter((a) => a.segments.length).length;
  const glyphs = request.blocks.reduce((n, b) => n + b.length, 0);

  const rows = [
    ['stage', stage ? `${out.stage} (${stage.name})` : String(out.stage)],
    [
      'rewritings',
      String((stage?.iterations ?? 0) + boost) +
        (boost ? ` (${stage?.iterations ?? 0} + ${boost} boost)` : ''),
    ],
    ['symbol', `${symbol.length} chars, ${count(symbol, 'F')} F, ${count(symbol, '[')} branches`],
    // The ride's length, and not an approximation of it: a branch springs OFF
    // the rail, so a vine follows its rail for exactly as many steps as the
    // symbol has F outside any bracket. Raising `follow steps` does nothing
    // until this outgrows it.
    ['trunk', `${trunkOf(symbol)} F outside a branch = rail steps`],
    // A mark comes back with no segments when it is not on its block's own
    // silhouette: nothing shapes the ring there, so there is no border of its
    // own for it to run.
    ['marks', `${out.anchors.length}, ${grew} grew, ${bordered} riding a border`],
    ['blocks', `${out.hulls.length}, ${glyphs} glyphs, ${out.hulls.map((h) => h.length).join('/') || '—'} corners`],
    // Measured, not asked for: on a narrow screen the page is capped by the
    // viewport whatever the column knob says, and the engine only ever sees
    // what was actually laid out.
    ['column', `${Math.round(request.host.w)} × ${Math.round(request.host.h)} px`],
    ['drawn', `${segments} segments, ${leaves} leaves`],
    ['obstacles', String(out.obstacles.length)],
    ['geometry scale', out.geometryScale.toFixed(3)],
    ['illuminate', `${ms.toFixed(2)} ms`],
  ];
  $('readout').innerHTML = rows
    .map(([k, v]) => `<dt>${k}</dt><dd>${escapeHtml(v)}</dd>`)
    .join('');

  const named = details
    .map((d) => d.text.slice(0, 6))
    .slice(0, 24)
    .join(' ');
  $('marks-readout').textContent = named || '(no marks on the page)';

  const shown =
    symbol.length > 6000 ? `${symbol.slice(0, 6000)}\n… ${symbol.length - 6000} more` : symbol;
  $('symbol').textContent = shown || '(nothing — a bare stage grows nothing at all)';
}

/** The symbol's trunk: its `F`s outside any branch. */
function trunkOf(symbol) {
  let depth = 0;
  let trunk = 0;
  for (const c of symbol) {
    if (c === '[') depth++;
    else if (c === ']') depth--;
    else if (c === 'F' && depth === 0) trunk++;
  }
  return trunk;
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
    .dbg-obstacle, .dbg-bounds, .dbg-anchor, .dbg-outline, .dbg-hull { display: none; }`;
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
