// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The measuring layer: the half of the contract the engine deliberately
// refuses to do.
//
// scriptorium measures nothing. It is handed rectangles a host has already
// laid out and hands path data back, which is what lets the same crate serve
// this page, a PDF renderer and a plotter. So everything DOM-shaped lives
// here: laying the prose out one term per span, reading those terms' boxes
// back, working out where each rendered CHARACTER puts ink, and deciding which
// of them are marks.
//
// Every rectangle produced here is relative to the COLUMN's top-left corner.
// That is the engine's own convention — `resolve_anchors` aims growth away
// from `host.w/2, host.h/2`, reading the host box as sitting at the origin —
// and it puts the page's own box at negative coordinates around the text,
// exactly as the crate's own example does.

import { inkMetrics, outlineFor } from './trace.js';
import { opcodeAt } from './notation.js';

/** The face a node is actually rendered in. */
export function faceOf(el) {
  const cs = getComputedStyle(el);
  return {
    style: cs.fontStyle,
    weight: cs.fontWeight,
    family: cs.fontFamily,
    size: parseFloat(cs.fontSize),
  };
}

/**
 * Lays the page out: one paragraph per block, one span per term.
 *
 * Per term, not per line: a line rectangle spans the full measure whether or
 * not the text reaches the end of it, and an obstacle field made of them would
 * leave a vine nowhere to be except outside the paragraph altogether.
 */
export function renderProse(columnEl, text, { dropCap = true } = {}) {
  columnEl.textContent = '';
  const paragraphs = text.split(/\n{2,}/).map((p) => p.trim()).filter(Boolean);
  let index = 0;

  paragraphs.forEach((para, block) => {
    const p = document.createElement('p');
    p.dataset.block = String(block);
    const term = (text, cls = 'term') => {
      const span = document.createElement('span');
      span.className = cls;
      span.dataset.index = String(index++);
      span.dataset.block = String(block);
      span.textContent = text;
      return span;
    };
    para.split(/\s+/).filter(Boolean).forEach((word, wi) => {
      // The drop cap is a term of its own, one character long. A
      // `::first-letter` has no box to measure and no box to grow off; this
      // one has both, and the rest of its word goes on being ordinary text to
      // be grown around.
      const chars = [...word];
      if (dropCap && block === 0 && wi === 0) {
        p.append(term(chars[0], 'term is-dropcap'));
        if (chars.length > 1) p.append(term(chars.slice(1).join('')));
      } else {
        p.append(term(word));
      }
      p.append(document.createTextNode(' '));
    });
    columnEl.append(p);
  });
  return [...columnEl.querySelectorAll('.term')];
}

function relTo(origin, r) {
  return { x: r.left - origin.left, y: r.top - origin.top, w: r.width, h: r.height };
}

/** Every client rectangle of a range, relative to the origin. */
function rangeRects(origin, node, start, end) {
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  return [...range.getClientRects()]
    .filter((r) => r.width > 0 && r.height > 0)
    .map((r) => relTo(origin, r));
}

/**
 * One layout pass: the page, the column, and every term's boxes, all in the
 * column's own coordinate space.
 */
export function measure(pageEl, columnEl, termEls) {
  const origin = columnEl.getBoundingClientRect();
  const pageRect = pageEl.getBoundingClientRect();
  const terms = termEls.map((el) => {
    const node = el.firstChild;
    const text = node ? node.textContent : '';
    return {
      el,
      text,
      block: Number(el.dataset.block),
      rects: node ? rangeRects(origin, node, 0, text.length) : [],
    };
  });
  return {
    origin,
    host: { x: 0, y: 0, w: origin.width, h: origin.height },
    page: relTo(origin, pageRect),
    terms,
  };
}

/**
 * Where each character of a term puts ink.
 *
 * Neither box the DOM will hand over is the ink. The span's own box carries
 * the line's leading above and below the letter; the advance carries the
 * side bearings either side of it. So the range rectangle is used only to
 * anchor the run — its left edge is where the first character's advance
 * starts, and the font box inside it fixes the baseline — and the ink itself
 * comes from canvas metrics, walked character by character along the advance.
 *
 * Kerning is not modelled. It moves a glyph a fraction of a pixel sideways
 * inside a word, and every use of this is a convex hull, where only the
 * outermost letters count at all.
 */
function glyphsOfTerm(origin, term) {
  const node = term.el.firstChild;
  if (!node || !term.text) return [];
  const [rect] = term.rects;
  if (!rect) return [];

  const face = faceOf(term.el);
  const glyphs = [];
  let x = rect.x;
  let k = 1;
  let baseline = rect.y;
  let first = true;

  for (const ch of term.text) {
    const m = inkMetrics(ch, face, face.size);
    if (first) {
      // The browser's inline content area and the canvas's font box are the
      // same measurement made twice; where they disagree slightly, scale the
      // metrics into agreement rather than letting the ink drift off the page.
      const fontBox = m.fontAscent + m.fontDescent;
      k = fontBox > 0 ? rect.h / fontBox : 1;
      baseline = rect.y + m.fontAscent * k;
      first = false;
    }
    const w = (m.left + m.right) * k;
    const h = (m.ascent + m.descent) * k;
    if (w > 0 && h > 0) {
      glyphs.push({
        ch,
        box: { x: x - m.left * k, y: baseline - m.ascent * k, w, h },
      });
    }
    x += m.advance * k;
  }
  return glyphs;
}

/**
 * The page's blocks, as the engine wants them: one entry per paragraph,
 * holding the characters it rendered, plus the outlines those characters need
 * for the hull to answer to the writing rather than to its boxes.
 */
export function blocksFrom(layout, { rideGlyph = true } = {}) {
  const blocks = [];
  const outlines = {};
  for (const term of layout.terms) {
    const block = Number.isFinite(term.block) ? term.block : 0;
    while (blocks.length <= block) blocks.push([]);
    const glyphs = glyphsOfTerm(layout.origin, term);
    blocks[block].push(...glyphs);
    if (!rideGlyph) continue;
    const face = faceOf(term.el);
    for (const g of glyphs) {
      if (g.ch in outlines) continue;
      const d = outlineFor(g.ch, face);
      if (d) outlines[g.ch] = d;
    }
  }
  return { blocks, outlines };
}

/**
 * Which terms are marks.
 *
 * The notation decides: a term opening with one of the enabled opcode marks is
 * a mark, and so is the drop cap, an initial being a mark by definition. A
 * click on any term overrides that either way, which is what the page is for.
 */
export function markedTerms(layout, marks, { dropCap = true, manual = new Map() } = {}) {
  const out = new Set();
  for (const term of layout.terms) {
    const key = term.el.dataset.index;
    const auto = (dropCap && term.el.classList.contains('is-dropcap')) || !!opcodeAt(term.text, marks);
    if (manual.has(key) ? manual.get(key) : auto) out.add(key);
  }
  return out;
}

/**
 * Turns the marked terms into seeds.
 *
 * How much of a term is the mark is the engine's own question, answered by
 * `markLeadLength`: a composite keeps all its parts (`¬⟨` is two characters,
 * `|·|` three), a mark wearing a count sheds it (`⧉₂` -> `⧉`), and a container
 * yields just its opening token. The drop cap overrides it to a single letter,
 * which is what a `::first-letter` is.
 */
export function seedsFrom(layout, marked, engine, { mode = 'edge', rideBlock = true } = {}) {
  const seeds = [];
  const details = [];
  for (const term of layout.terms) {
    if (!marked.has(term.el.dataset.index)) continue;
    const isDropCap = term.el.classList.contains('is-dropcap');
    const markLen = engine.markLeadLength(term.text, isDropCap ? 1 : null);
    if (!markLen) continue;

    const origin = layout.origin;
    const [markRect] = rangeRects(origin, term.el.firstChild, 0, markLen);
    if (!markRect) continue;
    const glyphs = glyphsOfTerm(origin, term);
    const ch = [...term.text][0];
    const inkBox = glyphs[0]?.box ?? null;

    seeds.push({
      box: term.rects[0] || markRect,
      markRect,
      mode,
      ch,
      inkBox,
      // Height, not width: a mark's rendered SIZE is its glyph height.
      size: markRect.h,
      block: rideBlock ? term.block : null,
    });
    details.push({ ch, inkBox, isDropCap, block: term.block, text: term.text });
  }
  return { seeds, details };
}

/**
 * The obstacle field: one unpadded rectangle per term, minus the marks
 * themselves.
 *
 * A vine grows off its own mark and along its outline, so it is inside that
 * box by construction — and an obstacle you start inside of is a trap rather
 * than a boundary.
 */
export function obstaclesFrom(layout, marked) {
  const out = [];
  for (const term of layout.terms) {
    if (marked.has(term.el.dataset.index)) continue;
    out.push(...term.rects);
  }
  return out;
}
