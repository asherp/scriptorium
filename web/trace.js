// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Glyph outlines, sampled from the host's own font.
//
// The engine takes outlines as SVG paths in a unit square and refuses to go
// looking for them itself — which character a vine can ride is a property of
// the host's notation and the host's font, not of the engine. A browser will
// not hand over a font's contours, but it will happily rasterize one, so this
// host gets its outlines the way a scribe would: it looks at the letter.
//
// Draw the character large on an offscreen canvas, threshold the alpha, follow
// the cracks between filled and empty pixels into closed loops, simplify, and
// normalize by the METRIC ink box rather than the traced pixel extent, so the
// unit square the engine is handed is exactly the box it gets mapped back onto
// on the page.

/** Render size for the trace. Big enough that a hairline serif is pixels wide. */
const RASTER_PX = 180;
/** Alpha at or above this counts as ink. */
const INK_ALPHA = 128;
/** Douglas-Peucker tolerance, in raster pixels. */
const SIMPLIFY_PX = 0.9;
/** Loops shorter than this many lattice steps are antialiasing crumbs. */
const MIN_LOOP = 12;

const cache = new Map();

/** A CSS font shorthand for one of the host's faces at a given size. */
export function fontString(face, px) {
  return `${face.style} ${face.weight} ${px}px ${face.family}`;
}

/**
 * Ink metrics for one character, in px at `px`: how far the glyph's ink
 * reaches from the drawing origin, which is the baseline at the start of the
 * advance.
 *
 * Memoized. A block's silhouette asks for this once per rendered character on
 * every layout pass — a page of prose is hundreds of calls a frame — and the
 * answer depends on nothing but the character and the face it is set in.
 */
const metricsCache = new Map();

export function inkMetrics(ch, face, px) {
  const key = `${face.style}|${face.weight}|${face.family}|${px}|${ch}`;
  let m = metricsCache.get(key);
  if (!m) {
    m = measureInk(ch, face, px);
    metricsCache.set(key, m);
  }
  return m;
}

function measureInk(ch, face, px) {
  const ctx = scratch();
  ctx.font = fontString(face, px);
  const m = ctx.measureText(ch);
  return {
    left: m.actualBoundingBoxLeft,
    right: m.actualBoundingBoxRight,
    ascent: m.actualBoundingBoxAscent,
    descent: m.actualBoundingBoxDescent,
    fontAscent: m.fontBoundingBoxAscent,
    fontDescent: m.fontBoundingBoxDescent,
    advance: m.width,
  };
}

/**
 * The character's outline as SVG path data in a unit square (x: 0..1
 * left-to-right, y: 0..1 top-to-bottom), or null where it has no ink to trace
 * — a space, or a character this font has no glyph for.
 *
 * Cached per (character, face), since the sampling is the expensive part and
 * does not depend on the size the glyph is finally drawn at.
 */
export function outlineFor(ch, face) {
  const key = `${face.style}|${face.weight}|${face.family}|${ch}`;
  if (!cache.has(key)) cache.set(key, trace(ch, face));
  return cache.get(key);
}

export function clearOutlineCache() {
  cache.clear();
  metricsCache.clear();
}

let scratchCanvas = null;
function scratch() {
  if (!scratchCanvas) scratchCanvas = document.createElement('canvas');
  return scratchCanvas.getContext('2d', { willReadFrequently: true });
}

function trace(ch, face) {
  const m = inkMetrics(ch, face, RASTER_PX);
  const inkW = m.left + m.right;
  const inkH = m.ascent + m.descent;
  if (!(inkW > 0.5) || !(inkH > 0.5)) return null;

  // A margin of empty on every side, so a glyph whose ink runs right to the
  // edge of its own box still has a crack to be followed round.
  const pad = 2;
  const w = Math.ceil(inkW) + pad * 2;
  const h = Math.ceil(inkH) + pad * 2;
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  ctx.font = fontString(face, RASTER_PX);
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';
  ctx.fillStyle = '#000';
  ctx.fillText(ch, pad + m.left, pad + m.ascent);

  const px = ctx.getImageData(0, 0, w, h).data;
  const bits = new Uint8Array(w * h);
  let any = false;
  for (let i = 0; i < w * h; i++) {
    if (px[i * 4 + 3] >= INK_ALPHA) {
      bits[i] = 1;
      any = true;
    }
  }
  if (!any) return null;

  const loops = followCracks(bits, w, h);
  if (!loops.length) return null;

  // Normalize by the metric box the engine will map back onto, NOT by the
  // traced extent: the two differ by an antialiased pixel or so, and a
  // mismatch slides the whole contour off the letter it is meant to ride.
  const parts = [];
  for (const loop of loops) {
    const simple = simplify(loop, SIMPLIFY_PX);
    if (simple.length < 3) continue;
    let d = '';
    for (let i = 0; i < simple.length; i++) {
      const ux = (simple[i][0] - pad) / inkW;
      const uy = (simple[i][1] - pad) / inkH;
      d += `${i === 0 ? 'M' : 'L'}${round(ux)},${round(uy)} `;
    }
    parts.push(d + 'Z');
  }
  return parts.length ? parts.join(' ') : null;
}

function round(v) {
  return Math.round(v * 10000) / 10000;
}

/**
 * Every closed boundary between ink and paper, as a lattice polygon.
 *
 * Each filled pixel contributes a directed edge for every side whose
 * neighbour is empty, wound so that ink stays on one hand; chaining those
 * edges head-to-tail walks each boundary — outer contours and counters alike
 * — exactly once. The engine drops the counters itself: a contour some larger
 * contour contains is a hole, and no vine rides a hole.
 */
function followCracks(bits, w, h) {
  const at = (x, y) => (x < 0 || y < 0 || x >= w || y >= h ? 0 : bits[y * w + x]);
  const key = (x, y) => y * (w + 1) + x;
  // Lattice vertex -> the ends of its outgoing edges. At most two, and only
  // where two ink corners meet diagonally is there more than one.
  const out = new Map();
  const push = (x0, y0, x1, y1) => {
    const k = key(x0, y0);
    const list = out.get(k);
    if (list) list.push([x1, y1]);
    else out.set(k, [[x1, y1]]);
  };

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (!at(x, y)) continue;
      if (!at(x, y - 1)) push(x, y, x + 1, y);
      if (!at(x + 1, y)) push(x + 1, y, x + 1, y + 1);
      if (!at(x, y + 1)) push(x + 1, y + 1, x, y + 1);
      if (!at(x - 1, y)) push(x, y + 1, x, y);
    }
  }

  const guard = w * h * 4;
  const loops = [];
  for (const start of [...out.keys()]) {
    const sx = start % (w + 1);
    const sy = (start - sx) / (w + 1);
    while (out.get(start)?.length) {
      const loop = [];
      let x = sx;
      let y = sy;
      // Every vertex has as many edges out as in, so a walk that consumes
      // edges as it goes must come back to where it started. The step cap is
      // there only so a malformed lattice can't hang the page.
      for (let i = 0; i < guard; i++) {
        const list = out.get(key(x, y));
        if (!list || !list.length) break;
        loop.push([x, y]);
        [x, y] = list.shift();
        if (x === sx && y === sy) break;
      }
      if (loop.length >= MIN_LOOP) loops.push(loop);
    }
  }
  return loops;
}

/** Douglas-Peucker, over a ring's vertices. */
function simplify(ring, tol) {
  if (ring.length < 4) return ring;
  const keep = new Uint8Array(ring.length);
  keep[0] = 1;
  keep[ring.length - 1] = 1;
  const stack = [[0, ring.length - 1]];
  while (stack.length) {
    const [a, b] = stack.pop();
    let far = -1;
    let best = tol;
    for (let i = a + 1; i < b; i++) {
      const d = pointLineDistance(ring[i], ring[a], ring[b]);
      if (d > best) {
        best = d;
        far = i;
      }
    }
    if (far > 0) {
      keep[far] = 1;
      stack.push([a, far], [far, b]);
    }
  }
  return ring.filter((_, i) => keep[i]);
}

function pointLineDistance(p, a, b) {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len = dx * dx + dy * dy;
  if (!len) return Math.hypot(p[0] - a[0], p[1] - a[1]);
  const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}
