// Geometry core derived from anidoodle (canvas-core/core.ts), Copyright 2026 Alex Greenshpun,
// licensed under the Apache License, Version 2.0 (full text: LICENSE-anidoodle in this directory).
// MODIFIED by the Borealis authors: ported from TypeScript to plain JavaScript and reduced to the pure
// helpers the sheet art needs (Apache-2.0 section 4b).
// Deterministic: all randomness comes from seeds. Points are [x, y] pairs.

export const rng = (seed) => {
  let a = seed >>> 0;
  return () => { a = (a + 0x6d2b79f5) | 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
};

const noiseCache = new Map();
const table = (seed) => {
  let t = noiseCache.get(seed);
  if (!t) {
    const r = rng(seed * 7919 + 13), perm = new Uint16Array(512), gx = new Float32Array(256), gy = new Float32Array(256);
    const p = Array.from({ length: 256 }, (_, i) => i);
    for (let i = 255; i > 0; i--) { const j = Math.floor(r() * (i + 1)); [p[i], p[j]] = [p[j], p[i]]; }
    for (let i = 0; i < 512; i++) perm[i] = p[i & 255];
    for (let i = 0; i < 256; i++) { const a = r() * Math.PI * 2; gx[i] = Math.cos(a); gy[i] = Math.sin(a); }
    t = { perm, gx, gy }; noiseCache.set(seed, t);
  }
  return t;
};
const noise2 = (seed, x, y) => {
  const { perm, gx, gy } = table(seed);
  const x0 = Math.floor(x), y0 = Math.floor(y), fx = x - x0, fy = y - y0;
  const g = (ix, iy, dx, dy) => { const h = perm[perm[ix & 255] + (iy & 255)]; return gx[h] * dx + gy[h] * dy; };
  const sx = fx * fx * (3 - 2 * fx), sy = fy * fy * (3 - 2 * fy);
  const a = g(x0, y0, fx, fy), b = g(x0 + 1, y0, fx - 1, fy), c = g(x0, y0 + 1, fx, fy - 1), d = g(x0 + 1, y0 + 1, fx - 1, fy - 1);
  return a + sx * (b - a) + sy * (c + sx * (d - c) - (a + sx * (b - a)));
};
export const fractal = (seed, x, y, fx, fy, oct) => {
  let sum = 0, amp = 1, kx = fx, ky = fy;
  for (let o = 0; o < oct; o++) { sum += noise2(seed + o * 101, x * kx, y * ky) * amp; amp *= 0.5; kx *= 2; ky *= 2; }
  return (sum + 1) / 2;
};

// Catmull-Rom resampling, `per` points per span
export const sample = (pts, closed = false, per = 10) => {
  if (pts.length < 2) return pts;
  const n = pts.length, segs = closed ? n : n - 1, out = [];
  const at = (i) => (closed ? pts[((i % n) + n) % n] : pts[Math.max(0, Math.min(n - 1, i))]);
  for (let i = 0; i < segs; i++) {
    const [p0, p1, p2, p3] = [at(i - 1), at(i), at(i + 1), at(i + 2)];
    for (let k = 0; k < per; k++) {
      const t = k / per, t2 = t * t, t3 = t2 * t;
      const c = (j) => 0.5 * (2 * p1[j] + (-p0[j] + p2[j]) * t + (2 * p0[j] - 5 * p1[j] + 4 * p2[j] - p3[j]) * t2 + (-p0[j] + 3 * p1[j] - 3 * p2[j] + p3[j]) * t3);
      out.push([c(0), c(1)]);
    }
  }
  out.push(closed ? out[0] : pts[n - 1]);
  return out;
};
export const oval = (cx, cy, rx, ry = rx, n = 10, a0 = -1.9) =>
  Array.from({ length: n }, (_, i) => { const a = a0 + (i / n) * Math.PI * 2; return [cx + Math.cos(a) * rx, cy + Math.sin(a) * ry]; });
export const arc = (cx, cy, rx, ry, a0, a1, n = 6) =>
  Array.from({ length: n }, (_, i) => { const a = a0 + (i / (n - 1)) * (a1 - a0); return [cx + Math.cos(a) * rx, cy + Math.sin(a) * ry]; });
// hand-inked wobble: push points through a smooth noise field
export const displace = (pts, amp, freq, oct, seed) =>
  pts.map(([x, y]) => [x + (fractal(seed, x, y, freq, freq, oct) - 0.5) * amp, y + (fractal(seed + 50, x, y, freq, freq, oct) - 0.5) * amp]);
