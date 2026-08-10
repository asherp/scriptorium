// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The measuring layer: the half of the contract the engine deliberately
// refuses to do.
//
// scriptorium measures nothing. It is handed rectangles a host has already
// laid out and hands path data back, which is what lets the same crate serve
// this page, a PDF renderer and a plotter. So everything DOM-shaped lives
// here: laying the prose out one term per span, reading those terms' boxes
// back, and working out where a mark's ink actually sits inside its line.
//
// Every rectangle produced here is relative to the COLUMN's top-left corner.
// That is the engine's own convention — `resolve_anchors` aims growth away
// from `host.w/2, host.h/2`, reading the host box as sitting at the origin —
// and it puts the page's own box at negative coordinates around the text,
// exactly as the crate's own example does.

import { inkMetrics, outlineFor } from './trace.js';

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
 * Lays the prose out as one span per term.
 *
 * Per term, not per line: a line rectangle spans the full measure whether or
 * not the text reaches the end of it, and a field made of them would leave a
 * vine nowhere to be except outside the paragraph altogether.
 */
export function renderProse(columnEl, text, { dropCap = true } = {}) {
  columnEl.textContent = '';
  const paragraphs = text.split(/\n{2,}/).map((p) => p.trim()).filter(Boolean);
  let index = 0;
  const term = (text, cls = 'term') => {
    const span = document.createElement('span');
    span.className = cls;
    span.dataset.index = String(index++);
    span.textContent = text;
    return span;
  };

  paragraphs.forEach((para, pi) => {
    const p = document.createElement('p');
    para.split(/\s+/).filter(Boolean).forEach((word, wi) => {
      // The drop cap is a term of its own, one character long. A
      // `::first-letter` has no box to measure and no box to grow off; this
      // one has both, and the rest of its word goes on being ordinary text to
      // be grown around.
      const chars = [...word];
      if (dropCap && pi === 0 && wi === 0) {
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
  const rects = [...range.getClientRects()].filter((r) => r.width > 0 && r.height > 0);
  return rects.map((r) => relTo(origin, r));
}

/**
 * Where a term's leading mark actually puts ink.
 *
 * Not the same box as the span's: an inline box carries the line's leading
 * above and below the letter, and the letter's own ink is inset from the
 * advance by its side bearings. The engine maps a unit-square outline
 * straight onto this box, so it has to be the ink and nothing else.
 *
 * The metrics come from a canvas measuring the same face; the range rectangle
 * anchors them on the page. Where the browser's inline content area and the
 * canvas's font box disagree slightly, the ratio between them scales the
 * metrics into agreement rather than letting the contour slide off the letter.
 */
export function inkBoxOf(origin, span, markLen) {
  const node = span.firstChild;
  if (!node || !node.textContent) return null;
  const text = node.textContent.slice(0, Math.max(1, markLen));
  const face = faceOf(span);
  const m = inkMetrics(text, face, face.size);
  const inkW = m.left + m.right;
  const inkH = m.ascent + m.descent;
  if (!(inkW > 0) || !(inkH > 0)) return null;

  const [rect] = rangeRects(origin, node, 0, Math.max(1, markLen));
  if (!rect) return null;
  const fontBox = m.fontAscent + m.fontDescent;
  const k = fontBox > 0 ? rect.h / fontBox : 1;
  const baseline = rect.y + m.fontAscent * k;
  return {
    x: rect.x - m.left * k,
    y: baseline - m.ascent * k,
    w: inkW * k,
    h: inkH * k,
  };
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
    return { el, text, rects: node ? rangeRects(origin, node, 0, text.length) : [] };
  });
  return {
    origin,
    host: { x: 0, y: 0, w: origin.width, h: origin.height },
    page: relTo(origin, pageRect),
    terms,
  };
}

/**
 * Turns the marked terms into seeds, and collects the outlines their marks
 * need.
 *
 * How much of a term is the mark is the engine's own question, answered by
 * `markLeadLength`: a composite keeps all its parts, a mark wearing a count
 * sheds it, and a container yields just its opening token. The drop cap
 * overrides it to a single letter, which is what a `::first-letter` is.
 */
export function seedsFrom(layout, marked, engine, { mode = 'edge', rideGlyph = true } = {}) {
  const seeds = [];
  const marks = [];
  const outlines = {};
  for (const term of layout.terms) {
    if (!marked.has(term.el.dataset.index)) continue;
    const isDropCap = term.el.classList.contains('is-dropcap');
    const markLen = engine.markLeadLength(term.text, isDropCap ? 1 : null);
    if (!markLen) continue;

    const origin = layout.origin;
    const [markRect] = rangeRects(origin, term.el.firstChild, 0, markLen);
    if (!markRect) continue;
    const box = term.rects[0] || markRect;
    const ch = [...term.text][0];
    const inkBox = inkBoxOf(origin, term.el, markLen);

    const face = faceOf(term.el);
    let outline = null;
    if (ch && rideGlyph) {
      outline = outlines[ch] ?? outlineFor(ch, face);
      if (outline) outlines[ch] = outline;
    }

    seeds.push({
      box,
      markRect,
      mode,
      ch,
      inkBox,
      // Height, not width: a mark's rendered SIZE is its glyph height.
      size: markRect.h,
    });
    marks.push({ ch, inkBox, outline, isDropCap });
  }
  return { seeds, marks, outlines };
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
