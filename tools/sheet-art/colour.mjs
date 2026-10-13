// Colour art set: the approved look (hand-inked ink strokes, gold accents, teal sky wash).
import { arc, oval, sample, displace } from "./geom.mjs";
import { blob, disc, line, pen, ringLine, star } from "./kit.mjs";

const ring = (cx, cy, r, w, _seed, tone = "ink") => ringLine(cx, cy, r, w, tone);
const seal = (seed) => [ring(50, 50, 45, 1.6, seed), ring(50, 50, 40.5, 0.9, seed + 1),
  ...[0, 1, 2, 3].map((i) => star(50 + 45 * Math.cos((i * Math.PI) / 2), 50 + 45 * Math.sin((i * Math.PI) / 2), 3.6, 1.4, "gold"))];

export const sigils = {
  str: () => [...seal(11), pen([[20, 70], [38, 38], [50, 56], [62, 34], [82, 70]], 4.2, 12), pen([[18, 73], [82, 73]], 2.2, 13, "ink", 0.8),
    blob(oval(36, 27, 5.5, 5.5, 18), 14, "gold")],
  dex: () => [...seal(21), pen([[28, 82], [72, 20]], 2.4, 22, "ink", 0.5),
    line([[36, 70], [34, 50], [50, 32], [72, 20], [66, 42], [52, 60], [36, 70]], 1.5, "ink", 23, 0.1),
    ...[0, 1, 2, 3].map((i) => { const t = 0.36 + i * 0.13, x = 28 + 44 * t, y = 82 - 62 * t; return pen([[x, y], [x - 9 + i * 1.5, y - 2]], 1, 24 + i, "ink", 0.7); }),
    ...[0, 1, 2].map((i) => { const t = 0.4 + i * 0.14, x = 28 + 44 * t, y = 82 - 62 * t; return pen([[x, y], [x + 7 - i, y + 3]], 1, 34 + i, "ink", 0.7); }),
    star(72, 20, 5, 1.8, "gold")],
  con: () => [...seal(41), line([[32, 26], [68, 26], [69, 50], [62, 66], [50, 76], [38, 66], [31, 50], [32, 26]], 3.6, "ink", 42, 0.2),
    pen([[50, 30], [50, 70]], 1.3, 43, "ink", 0.6), star(50, 46, 8, 3.4, "gold")],
  int: () => [...seal(51), pen(arc(50, 62, 28, 22, -Math.PI * 0.86, -Math.PI * 0.14, 20), 3, 52, "ink", 0.7),
    pen(arc(50, 38, 28, 22, Math.PI * 0.14, Math.PI * 0.86, 20), 3, 53, "ink", 0.7),
    disc(50, 50, 7.5, "gold"), disc(50, 50, 2.6, "ink"),
    pen([[50, 30], [50, 18]], 1.6, 56, "ink", 0.8), pen([[38, 32], [32, 22]], 1.4, 57, "ink", 0.8), pen([[62, 32], [68, 22]], 1.4, 58, "ink", 0.8)],
  wis: () => [...seal(61),
    blob([...arc(48, 50, 26, 26, Math.PI * 0.32, Math.PI * 1.68, 28), ...arc(58, 46, 21, 21, Math.PI * 1.55, Math.PI * 0.45, 24).slice(1, -1)], 62),
    star(62, 46, 9, 3, "gold"), star(70, 66, 4, 1.4, "gold")],
  cha: () => [...seal(71), disc(50, 50, 10, "gold"), ring(50, 50, 14, 1.2, 73),
    ...Array.from({ length: 8 }, (_, i) => { const a = (i * Math.PI) / 4 - Math.PI / 2, r0 = 18, r1 = i % 2 ? 28 : 34;
      return pen([[50 + Math.cos(a) * r0, 50 + Math.sin(a) * r0], [50 + Math.cos(a) * r1, 50 + Math.sin(a) * r1]], 3, 74 + i, "ink", 0.9); })],
};

export const corner = () => [
  pen([[14, 116], [14, 26], [20, 18], [26, 14], [116, 14]], 2.2, 81, "ink", 0.85),
  pen([[22, 100], [22, 30], [30, 22], [100, 22]], 0.8, 82, "ink", 0.85),
  pen(arc(14, 14, 30, 30, Math.PI * 0.08, Math.PI * 0.42, 16), 1.3, 83, "gold", 0.9, 0.2),
  pen(arc(14, 14, 40, 40, Math.PI * 0.12, Math.PI * 0.38, 16), 0.8, 84, "ink", 0.9, 0.2),
  ...[48, 56, 63].map((r, k) => disc(14 + r * 0.7071, 14 + r * 0.7071, 1.5 - k * 0.3, k ? "ink" : "gold")),
  star(14, 14, 11, 3.2, "gold", 4, -Math.PI / 4), star(14, 14, 7, 2.2, "gold"),
  disc(120, 14, 1.8, "gold"), disc(14, 120, 1.8, "gold"),
];

export const divider = () => [
  pen([[184, 10], [8, 10]], 1.5, 91, "ink", 1, 0.25), pen([[216, 10], [392, 10]], 1.5, 92, "ink", 1, 0.25),
  star(200, 10, 8, 2.4, "gold"), star(184, 10, 2.6, 1.1, "ink"), star(216, 10, 2.6, 1.1, "ink"),
];
export const headerRule = () => [star(6, 6, 5.2, 1.7, "gold"), pen([[14, 6], [600, 6]], 1.2, 102, "ink", 0.15, 0.2)];

export const frame = (W, H, seed) => {
  const o = 20, i = 25;
  const edge = (a, b, w, s) => pen([a, b], w, s, "ink", 0, 0.35);
  return [edge([o + 30, o], [W - o - 30, o], 1.1, seed), edge([o + 30, H - o], [W - o - 30, H - o], 1.1, seed + 1),
    edge([o, o + 30], [o, H - o - 30], 1.1, seed + 2), edge([W - o, o + 30], [W - o, H - o - 30], 1.1, seed + 3),
    edge([i + 26, i], [W - i - 26, i], 0.4, seed + 4), edge([i + 26, H - i], [W - i - 26, H - i], 0.4, seed + 5),
    edge([i, i + 26], [i, H - i - 26], 0.4, seed + 6), edge([W - i, i + 26], [W - i, H - i - 26], 0.4, seed + 7)];
};

const curtain = (y0, amp, ph, h, seed) => {
  const top = Array.from({ length: 30 }, (_, k) => { const x = (k / 29) * 612; return [x, y0 + Math.sin(x / 95 + ph) * amp + Math.sin(x / 37 + ph * 2) * amp * 0.25]; });
  const bot = top.map(([x, y]) => [x, y + h + Math.sin(x / 60 + ph) * h * 0.25]).reverse();
  return displace([...top, ...bot], 2.5, 0.02, 2, seed);
};
const WATERMARK = 0.3; // baked in: the ribbon is faint on its own, draw it as-is
// flat alpha bands only (no gradients) so the PDF writer needs just ExtGState alpha
export const ribbon = () => {
  const bands = [[40, 26, 0.3, 90, 111], [70, 18, 2.1, 70, 112], [20, 14, 4.0, 120, 113]];
  return bands.flatMap(([y, amp, ph, h, seed], k) => [
    { ...blob(curtain(y, amp, ph, h, seed), seed, "wash", 0, 1, 0.6), alpha: [0.5, 0.35, 0.25][k] * WATERMARK },
    { ...blob(curtain(y + h * 0.75, amp, ph, h * 0.35, seed + 5), seed + 5, "gold", 0, 1, 0.6), alpha: [0.35, 0.25, 0.2][k] * WATERMARK },
  ]);
};
