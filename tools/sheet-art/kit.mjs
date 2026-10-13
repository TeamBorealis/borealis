// Drawing primitives. Every primitive returns an Item:
//   { ops: [["M",x,y],["L",x,y],["C",x1,y1,x2,y2,x,y],["Z"]...], fill?: tone, stroke?: tone, w?: width, alpha?: 0..1 }
// Tones are symbolic ("ink","gold","wash","fine","mid","light"); each art set maps them to RGB.
import { displace, sample } from "./geom.mjs";

const r2 = (n) => Math.round(n * 100) / 100;

// Ramer-Douglas-Peucker on an open polyline
const rdp = (pts, eps) => {
  if (pts.length < 3) return pts;
  const [a, b] = [pts[0], pts[pts.length - 1]];
  let idx = 0, dmax = 0;
  const dx = b[0] - a[0], dy = b[1] - a[1], len = Math.hypot(dx, dy);
  for (let i = 1; i < pts.length - 1; i++) {
    const d = len ? Math.abs(dy * pts[i][0] - dx * pts[i][1] + b[0] * a[1] - b[1] * a[0]) / len : Math.hypot(pts[i][0] - a[0], pts[i][1] - a[1]);
    if (d > dmax) { dmax = d; idx = i; }
  }
  if (dmax <= eps) return [a, b];
  return [...rdp(pts.slice(0, idx + 1), eps).slice(0, -1), ...rdp(pts.slice(idx), eps)];
};
const simplify = (pts, closed, eps) => {
  if (!closed) return rdp(pts, eps);
  const s = rdp([...pts, pts[0]], eps);
  return s.slice(0, -1);
};

// Catmull-Rom spline through the points, as exact cubic Beziers
export const spline = (pts, closed = false) => {
  const n = pts.length;
  if (n < 2) return [];
  const at = (i) => (closed ? pts[((i % n) + n) % n] : pts[Math.max(0, Math.min(n - 1, i))]);
  const ops = [["M", ...pts[0]]];
  const segs = closed ? n : n - 1;
  for (let i = 0; i < segs; i++) {
    const [p0, p1, p2, p3] = [at(i - 1), at(i), at(i + 1), at(i + 2)];
    const cap = (vx, vy, lim) => { const m = Math.hypot(vx, vy); return m > lim ? [(vx * lim) / m, (vy * lim) / m] : [vx, vy]; };
    const lim = Math.hypot(p2[0] - p1[0], p2[1] - p1[1]) / 2.5; // handles never outrun their own span: no overshoot on short edges
    const [ax, ay] = cap((p2[0] - p0[0]) / 6, (p2[1] - p0[1]) / 6, lim), [bx, by] = cap((p3[0] - p1[0]) / 6, (p3[1] - p1[1]) / 6, lim);
    ops.push(["C", p1[0] + ax, p1[1] + ay, p2[0] - bx, p2[1] - by, p2[0], p2[1]]);
  }
  if (closed) ops.push(["Z"]);
  return ops;
};
export const polyline = (pts, closed = false) => [["M", ...pts[0]], ...pts.slice(1).map((p) => ["L", ...p]), ...(closed ? [["Z"]] : [])];

// hand-inked centreline: sample, wobble, simplify, spline
export const inked = (pts, amp = 0.4, seed = 1, closed = false, eps = 0.1) => {
  const s = sample(pts, closed, 6);
  const body = closed ? s.slice(0, -1) : s;
  const w = amp > 0 ? displace(body, amp, 0.06, 2, seed) : body;
  return simplify(w, closed, eps);
};

// pen stroke centreline -> tapered ribbon outline (taper 0 = even line, 1 = brush swell)
const ribbonOutline = (c, w, taper) => {
  const L = [], R = [];
  c.forEach((p, i) => {
    const t = i / (c.length - 1), q = c[Math.min(c.length - 1, i + 1)], o = c[Math.max(0, i - 1)];
    const dx = q[0] - o[0], dy = q[1] - o[1], len = Math.hypot(dx, dy) || 1;
    const ends = Math.min(1, t / 0.18, (1 - t) / 0.18), r = (w / 2) * (1 - taper + taper * Math.pow(Math.max(0.08, ends), 0.6));
    L.push([p[0] - (dy / len) * r, p[1] + (dx / len) * r]); R.push([p[0] + (dy / len) * r, p[1] - (dx / len) * r]);
  });
  return [...L, ...R.reverse()];
};

// ---- items
/** filled pen-stroke ribbon */
export const pen = (c, w, seed, tone = "ink", taper = 0.6, amp = 0.5, alpha = 1) => {
  const centre = sample(amp > 0 ? displace(sample(c, false, 6), amp, 0.06, 2, seed) : sample(c, false, 6), false, 1);
  const out = simplify(ribbonOutline(centre, w, taper), true, 0.12);
  return { ops: spline(out, true), fill: tone, alpha };
};
/** filled organic blob through the points */
export const blob = (pts, seed, tone = "ink", amp = 0.4, alpha = 1, eps = 0.1) => ({ ops: spline(inked(pts, amp, seed, true, eps), true), fill: tone, alpha });
/** crisp filled polygon */
export const poly = (pts, tone = "ink", alpha = 1) => ({ ops: polyline(pts, true), fill: tone, alpha });
export const starPts = (cx, cy, R, r, n = 4, rot = -Math.PI / 2) =>
  Array.from({ length: n * 2 }, (_, i) => { const a = rot + (i * Math.PI) / n, k = i % 2 ? r : R; return [cx + Math.cos(a) * k, cy + Math.sin(a) * k]; });
export const star = (cx, cy, R, r, tone = "gold", n = 4, rot = -Math.PI / 2, alpha = 1) => poly(starPts(cx, cy, R, r, n, rot), tone, alpha);
/** true circle from four cubics */
export const circleOps = (cx, cy, r) => {
  const k = 0.5522847498 * r;
  return [["M", cx + r, cy], ["C", cx + r, cy + k, cx + k, cy + r, cx, cy + r], ["C", cx - k, cy + r, cx - r, cy + k, cx - r, cy],
    ["C", cx - r, cy - k, cx - k, cy - r, cx, cy - r], ["C", cx + k, cy - r, cx + r, cy - k, cx + r, cy], ["Z"]];
};
export const disc = (cx, cy, r, tone = "gold", alpha = 1) => ({ ops: circleOps(cx, cy, r), fill: tone, alpha });
export const ringLine = (cx, cy, r, w, tone = "ink", alpha = 1) => ({ ops: circleOps(cx, cy, r), stroke: tone, w, alpha });
/** stroked spline through points (hand-inked unless amp = 0) */
export const line = (pts, w, tone = "ink", seed = 1, amp = 0, alpha = 1, closed = false) =>
  ({ ops: spline(amp > 0 || pts.length > 2 ? inked(pts, amp, seed, closed, 0.06) : pts, closed), stroke: tone, w, alpha });
/** stroked straight polyline (exact corners) */
export const seg = (pts, w, tone = "ink", alpha = 1, closed = false) => ({ ops: polyline(pts, closed), stroke: tone, w, alpha });
/** many disjoint straight strokes in one path: [[a,b],...] */
export const strokes = (pairs, w, tone = "ink", alpha = 1) => ({ ops: pairs.flatMap(([a, b]) => [["M", ...a], ["L", ...b]]), stroke: tone, w, alpha });
/** many small filled dots in one path */
export const dots = (pts, r, tone = "ink", alpha = 1) => ({ ops: pts.flatMap(([x, y]) => circleOps(x, y, r)), fill: tone, alpha });

export const deg = (a) => (a * Math.PI) / 180;
export const polar = (cx, cy, r, a) => [cx + Math.cos(a) * r, cy + Math.sin(a) * r];

// ---- serialisation
export const roundOps = (ops) => ops.map((o) => [o[0], ...o.slice(1).map(r2)]);
export const opsToD = (ops) => ops.map((o) => o[0] + o.slice(1).map(r2).join(" ")).join("");
export const bbox = (ops) => {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const o of ops) for (let i = 1; i < o.length; i += 2) { x0 = Math.min(x0, o[i]); x1 = Math.max(x1, o[i]); y0 = Math.min(y0, o[i + 1]); y1 = Math.max(y1, o[i + 1]); }
  return { x0, y0, x1, y1 };
};

// ---- generic helpers
/** parallel hatch lines clipped to a polygon (scanline). angle in degrees, gap in units */
export const hatch = (poly, angle, gap, w, tone = "ink", alpha = 1, inset = 0) => {
  const a = deg(angle), c = Math.cos(a), s = Math.sin(a);
  const rot = poly.map(([x, y]) => [x * c + y * s, -x * s + y * c]);
  const ys = rot.map((p) => p[1]), pairs = [];
  for (let y = Math.ceil(Math.min(...ys) / gap) * gap + gap / 2; y < Math.max(...ys); y += gap) {
    const xs = [];
    for (let i = 0; i < rot.length; i++) {
      const [p, q] = [rot[i], rot[(i + 1) % rot.length]];
      if ((p[1] <= y && q[1] > y) || (q[1] <= y && p[1] > y)) xs.push(p[0] + ((y - p[1]) / (q[1] - p[1])) * (q[0] - p[0]));
    }
    xs.sort((m, n) => m - n);
    for (let k = 0; k + 1 < xs.length; k += 2) {
      const [x0, x1] = [xs[k] + inset, xs[k + 1] - inset];
      if (x1 - x0 > 0.8) pairs.push([[x0 * c - y * s, x0 * s + y * c], [x1 * c - y * s, x1 * s + y * c]]);
    }
  }
  return strokes(pairs, w, tone, alpha);
};
/** transform every coordinate pair of items */
export const xform = (items, fn) => items.map((it) => ({ ...it, ops: it.ops.map((o) => [o[0], ...o.slice(1).flatMap((_, i, a) => (i % 2 ? [] : fn(a[i], a[i + 1])))]) }));
export const mirrorDiag = (items) => xform(items, (x, y) => [y, x]);
/** constellation: stars as dots, links as hairlines */
export const constellation = (pts, links, w, r, tone = "ink", alpha = 1) => [
  strokes(links.map(([i, j]) => [pts[i], pts[j]]), w, tone, alpha), dots(pts, r, tone, 1)];
/** dotted arc */
export const dotArc = (cx, cy, rad, a0, a1, n, r, tone = "ink", alpha = 1) =>
  dots(Array.from({ length: n }, (_, i) => polar(cx, cy, rad, a0 + ((a1 - a0) * i) / Math.max(1, n - 1))), r, tone, alpha);
