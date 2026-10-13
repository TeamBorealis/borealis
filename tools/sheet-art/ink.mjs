// Ink (monochrome) art set: black and greys only, richer than the colour set to make up for the missing colour:
// rune-tick rings, constellations, fine hatching and filigree. Accent "gold" collapses to solid black.
import { arc, oval, rng } from "./geom.mjs";
import { blob, circleOps, poly, spline, constellation, deg, disc, dotArc, dots, hatch, line, mirrorDiag, pen, polar, ringLine, seg, star, starPts, strokes, xform } from "./kit.mjs";

const TAU = Math.PI * 2;
// clean volute: radius shrinks to a fifth over `turns` turns
const spiral = (cx, cy, r0, turns, a0, dir = 1) => Array.from({ length: 22 }, (_, i) => { const t = i / 21; return polar(cx, cy, r0 * (1 - 0.8 * t), a0 + dir * t * turns * TAU); });

// seal: three rings, a ring of rune ticks, cardinal diamonds and diagonal studs
const seal = () => {
  const ticks = Array.from({ length: 48 }, (_, i) => { const a = (i / 48) * TAU, long = i % 2 === 0; return [polar(50, 50, long ? 41.7 : 42.6, a), polar(50, 50, 44.3, a)]; });
  return [ringLine(50, 50, 45.5, 1.6), ringLine(50, 50, 41, 0.9), ringLine(50, 50, 37.2, 0.9, "mid"), strokes(ticks, 0.9, "ink"),
    ...[0, 1, 2, 3].map((i) => star(...polar(50, 50, 45.5, (i * Math.PI) / 2), 4.2, 1.5, "ink")),
    dots([0, 1, 2, 3].map((i) => polar(50, 50, 45.5, Math.PI / 4 + (i * Math.PI) / 2)), 1.2, "ink")];
};
const stars = (pts, links) => constellation(pts, links, 0.9, 1.15, "mid", 1);

export const sigils = {
  str: () => [...seal(),
    ...stars([[24, 36], [30, 26], [20, 28]], [[0, 1], [1, 2]]), ...stars([[78, 30], [70, 24]], [[0, 1]]),
    pen([[20, 70], [38, 38], [50, 56], [62, 34], [82, 70]], 3.6, 12), seg([[18, 73], [82, 73]], 1.6, "ink"),
    hatch([[39, 40], [49, 56], [44, 71], [30, 71], [36, 52]], 62, 3.1, 0.9, "mid", 1),
    hatch([[62, 37], [52, 56], [57, 71], [74, 71], [68, 52]], 62, 3.1, 0.9, "mid", 1),
    seg([[34, 46], [38, 49], [42, 45], [46, 51]], 0.9, "ink"), seg([[58, 42], [62, 46], [66, 41], [69, 46]], 0.9, "ink"),
    seg([[30, 78], [70, 78]], 0.9, "mid"), seg([[40, 82], [60, 82]], 0.9, "mid"),
    disc(50, 25, 4.6, "ink"), ringLine(50, 25, 7.4, 0.9), strokes(Array.from({ length: 8 }, (_, i) => [polar(50, 25, 9.4, (i * TAU) / 8), polar(50, 25, 12.4, (i * TAU) / 8)]), 0.9)],
  dex: () => {
    const vane = [[36, 70], [34, 50], [50, 32], [72, 20], [66, 42], [52, 60]];
    return [...seal(), ...stars([[22, 28], [30, 20], [38, 24]], [[0, 1], [1, 2]]), ...stars([[78, 66], [72, 76], [82, 78]], [[0, 1], [1, 2]]),
      pen([[28, 82], [72, 20]], 2.2, 22, "ink", 0.5), line([...vane, [36, 70]], 1.5, "ink", 23, 0.1, 1, false),
      hatch(vane, 58, 2.6, 0.9, "mid", 1, 0.8),
      ...[0, 1, 2, 3].map((i) => { const t = 0.36 + i * 0.13, x = 28 + 44 * t, y = 82 - 62 * t; return pen([[x, y], [x - 9 + i * 1.5, y - 2]], 1, 24 + i, "ink", 0.7); }),
      disc(24, 84, 2.3, "ink"), disc(18, 78, 1.4, "ink"), disc(32, 86, 1.2, "ink"),
      star(72, 20, 5, 1.8, "ink")];
  },
  con: () => {
    const outline = [[32, 26], [68, 26], [69, 50], [62, 66], [50, 76], [38, 66], [31, 50], [32, 26]];
    const inner = outline.map(([x, y]) => [50 + (x - 50) * 0.76, 50 + (y - 50) * 0.76 + 1.5]);
    return [...seal(), ...stars([[22, 30], [28, 22]], [[0, 1]]), ...stars([[76, 24], [82, 32], [72, 18]], [[0, 1], [0, 2]]),
      line(outline, 3.2, "ink", 42, 0.2), seg(inner, 1, "ink", 1, true),
      hatch([[50, 28.5], [66, 28.5], [66.4, 49], [50, 49]], 45, 2.8, 0.9, "mid", 1, 0.6),
      hatch([[34, 51], [50, 51], [50, 72], [41, 66], [34.2, 51]], 45, 2.8, 0.9, "mid", 1, 0.6),
      seg([[50, 28], [50, 72]], 1, "ink"), seg([[33, 49]].concat([[67, 49]]), 1, "ink"),
      star(50, 49, 8, 3.2, "ink"), ringLine(50, 49, 10.2, 0.9),
      dots([[36, 31], [64, 31], [34, 53], [66, 53]], 1.3, "ink")];
  },
  int: () => [...seal(), ...stars([[26, 70], [34, 78], [22, 80]], [[0, 1], [0, 2]]), ...stars([[74, 72], [80, 80]], [[0, 1]]),
    pen(arc(50, 62, 28, 22, -Math.PI * 0.86, -Math.PI * 0.14, 20), 2.8, 52, "ink", 0.7),
    pen(arc(50, 38, 28, 22, Math.PI * 0.14, Math.PI * 0.86, 20), 2.8, 53, "ink", 0.7),
    ringLine(50, 50, 11.5, 0.9), ringLine(50, 50, 8.4, 1.1), disc(50, 50, 4.2, "ink"), disc(47.6, 47.8, 1.3, "mid"),
    strokes(Array.from({ length: 16 }, (_, i) => [polar(50, 50, 8.9, (i * TAU) / 16), polar(50, 50, 11.1, (i * TAU) / 16)]), 0.8, "mid"),
    strokes([[[50, 30], [50, 17]], [[38, 32], [32, 21]], [[62, 32], [68, 21]], [[27, 38], [18, 30]], [[73, 38], [82, 30]]].map(([a, b]) => [a, b]), 1.2, "ink"),
    dots([[50, 13.6], [29.6, 17.5], [70.4, 17.5]], 1.2, "ink"),
    dotArc(50, 50, 31, deg(150), deg(30), 9, 0.9, "mid")],
  wis: () => {
    const crescent = [...arc(48, 50, 26, 26, Math.PI * 0.32, Math.PI * 1.68, 28), ...arc(58, 46, 21, 21, Math.PI * 1.55, Math.PI * 0.45, 24).slice(1, -1)];
    return [...seal(), blob(crescent, 62, "ink", 0.2), hatch(crescent.slice(0, -1), 20, 2.5, 0.9, "light", 1, 3.2),
      star(64, 44, 9, 3, "ink"), ringLine(64, 44, 12.5, 0.9),
      ...stars([[72, 66], [80, 54], [70, 30], [82, 40]], [[0, 1], [1, 3], [3, 2]]),
      star(72, 66, 4.2, 1.4, "ink"), star(80, 54, 3, 1, "ink"),
      dotArc(50, 50, 33, deg(200), deg(330), 10, 0.9, "mid"),
      strokes([[[26, 28], [32, 30]], [[22, 36], [27, 37]], [[24, 70], [30, 68]]], 0.9, "mid")];
  },
  cha: () => {
    const rays = Array.from({ length: 8 }, (_, i) => { const a = (i * TAU) / 8 - Math.PI / 2, r1 = i % 2 ? 28 : 34; return pen([polar(50, 50, 18, a), polar(50, 50, r1, a)], 2.8, 74 + i, "ink", 0.9); });
    const small = Array.from({ length: 8 }, (_, i) => { const a = (i * TAU) / 8 - Math.PI / 2 + Math.PI / 8; return [polar(50, 50, 18.5, a), polar(50, 50, 24.5, a)]; });
    const tips = Array.from({ length: 4 }, (_, i) => star(...polar(50, 50, 31.5, (i * Math.PI) / 2 + Math.PI / 4), 3.2, 1.1, "ink"));
    return [...seal(), disc(50, 50, 7, "ink"), ringLine(50, 50, 10.4, 1), ringLine(50, 50, 13.6, 0.9, "mid"), dotArc(50, 50, 16, 0, TAU * 0.97, 24, 0.55, "ink"),
      ...rays, strokes(small, 1, "mid"), ...tips];
  },
};

// ---- corner (124 x 124, top-left; consumers mirror it for the other three)
export const corner = () => {
  const radial = Array.from({ length: 9 }, (_, i) => { const a = deg(10 + i * 8.75); return [polar(14, 14, 14, a), polar(14, 14, i % 2 ? 24 : 29, a)]; });
  const arm = [
    pen([[14, 116], [14, 26], [20, 18], [26, 14], [116, 14]], 2.2, 81, "ink", 0.85),
    pen([[22, 100], [22, 30], [30, 22], [100, 22]], 0.9, 82, "ink", 0.85),
    strokes(Array.from({ length: 14 }, (_, i) => [[44 + i * 5.2, 16.2], [44 + i * 5.2, 19.8]]), 0.8, "mid"),
    line(spiral(60, 35, 8, 2.1, -1.2), 1, "ink", 85, 0), line(spiral(88, 33, 5.5, 1.8, 3.4, -1), 0.9, "mid", 86, 0),
    ...constellation([[60, 52], [72, 46], [84, 54], [96, 44]], [[0, 1], [1, 2], [2, 3]], 0.9, 1.4, "mid"),
    star(110, 18, 4.6, 1.5, "ink", 4), disc(122, 18, 1.2, "ink"),
  ];
  return [...arm, ...mirrorDiag(arm.slice(2)),
    pen(arc(14, 14, 34, 34, deg(6), deg(84), 16), 1.2, 83, "ink", 0.9, 0.2), pen(arc(14, 14, 42, 42, deg(10), deg(80), 16), 0.9, 84, "mid", 0.9, 0.2),
    dotArc(14, 14, 50, deg(8), deg(82), 9, 1.0, "ink"), dotArc(14, 14, 58, deg(12), deg(78), 5, 0.9, "mid"),
    strokes(radial, 0.9, "ink"), star(14, 14, 12.5, 3.4, "ink", 4, -Math.PI / 4), star(14, 14, 8, 2.4, "ink"), ringLine(14, 14, 13.4, 0.9, "ink")];
};

// ---- divider and header rule
export const divider = () => {
  const chain = [-1, 1].flatMap((sgn) => [[32, 3.2], [48, 2.2]].map(([dx, r]) => star(200 + sgn * dx, 10, r, r * 0.38, "ink")));
  return [pen([[184, 10], [30, 10]], 1.5, 91, "ink", 1, 0.2), pen([[216, 10], [370, 10]], 1.5, 92, "ink", 1, 0.2),
    pen([[170, 5.6], [60, 5.6]], 0.9, 93, "mid", 1, 0), pen([[230, 5.6], [340, 5.6]], 0.9, 94, "mid", 1, 0), pen([[170, 14.4], [60, 14.4]], 0.9, 95, "mid", 1, 0), pen([[230, 14.4], [340, 14.4]], 0.9, 96, "mid", 1, 0),
    star(200, 10, 9, 2.6, "ink"), ringLine(200, 10, 6, 0.9), ...chain,
    line([[30, 10], [22, 7], [16, 10], [20, 13], [24, 10]], 1, "ink", 97, 0), line([[370, 10], [378, 7], [384, 10], [380, 13], [376, 10]], 1, "ink", 98, 0), disc(8, 10, 1.4, "ink"), disc(392, 10, 1.4, "ink")];
};
export const headerRule = () => [star(6, 6, 5.6, 1.8, "ink"), pen([[14, 6], [596, 6]], 1.1, 102, "ink", 0.15, 0.15),
  strokes(Array.from({ length: 48 }, (_, i) => [[22 + i * 12, 8.8], [22 + i * 12, 11]]), 0.9, "mid"), disc(596, 6, 1.4, "ink")];

// ---- page frame: double rule with an interlaced cord between the rules, diamond knots and mid-edge medallions
// one half-wave per cubic: crest-to-crest cord strand along x at centre line y, amplitude amp
const cord = (x0, x1, y, amp, sign, half) => {
  const n = Math.max(1, Math.round((x1 - x0) / half)), L = (x1 - x0) / n, ops = [["M", x0, y]];
  for (let k = 0; k < n; k++) { const s = (k % 2 ? -1 : 1) * sign * amp * 1.32, xa = x0 + k * L; ops.push(["C", xa + 0.3642 * L, y + s, xa + 0.6358 * L, y + s, xa + L, y]); }
  return { ops, stroke: "ink", w: 0.45, alpha: 1, n, L };
};
const knotDiamonds = (x0, n, L, y, R, every) => Array.from({ length: Math.floor(n / every) + 1 }, (_, m) => m * every).filter((k) => k <= n).map((k) => poly(starPts(x0 + k * L, y, R, R, 2, 0), "ink"));
export const frame = (W, H, seed) => {
  const o = 20, i = 25, g = 30, c = (o + i) / 2, HALF = 11, A = 1.5;
  const edge = (a, b, w, s, tone = "ink") => pen([a, b], w, s, tone, 0, 0.3);
  const swap = (items) => xform(items, (x, y) => [y, x]);
  const mid = (cx, cy) => [star(cx, cy, 8, 2.5, "ink", 4, cx === W / 2 ? -Math.PI / 2 : 0), star(cx - (cx === W / 2 ? 14 : 0), cy - (cx === W / 2 ? 0 : 14), 2.8, 1, "ink"), star(cx + (cx === W / 2 ? 14 : 0), cy + (cx === W / 2 ? 0 : 14), 2.8, 1, "ink")];
  // one run of cord (two opposed strands) plus knots, horizontal at y
  const run = (x0, x1, y) => { const a = cord(x0, x1, y, A, 1, HALF), b = cord(x0, x1, y, A, -1, HALF); return [a, b, ...knotDiamonds(x0, a.n, a.L, y, 2.1, 4)]; };
  const runs = (len, y, horizontal) => {
    const ends = [[o + 40, len / 2 - 20], [len / 2 + 21, len - o - 40]];
    const items = ends.flatMap(([x0, x1]) => run(x0, x1, y));
    return horizontal ? items : swap(items);
  };
  return [edge([o + g, o], [W - o - g, o], 1.1, seed), edge([o + g, H - o], [W - o - g, H - o], 1.1, seed + 1),
    edge([o, o + g], [o, H - o - g], 1.1, seed + 2), edge([W - o, o + g], [W - o, H - o - g], 1.1, seed + 3),
    edge([i + 26, i], [W - i - 26, i], 0.5, seed + 4), edge([i + 26, H - i], [W - i - 26, H - i], 0.5, seed + 5),
    edge([i, i + 26], [i, H - i - 26], 0.5, seed + 6), edge([W - i, i + 26], [W - i, H - i - 26], 0.5, seed + 7),
    ...runs(W, c, true), ...runs(W, H - c, true), ...runs(H, c, false), ...runs(H, W - c, false),
    ...mid(W / 2, o), ...mid(W / 2, H - o), ...mid(o, H / 2), ...mid(W - o, H / 2)];
};

// ---- borealis ribbon as grey line work. The layout squashes this piece vertically (to ~1/11 in ink mode), which
// crushes horizontal stroke widths, so the art is FILLED shapes only: swelling, tapering strands plus a sparse,
// irregular picket of tapering "curtain" rays that rise from each strand and fade with height. No lattice.
const strandY = (y0, amp, ph, x) => y0 + Math.sin(x / 95 + ph) * amp + Math.sin(x / 37 + ph * 2) * amp * 0.25;
const smooth = (t) => t * t * (3 - 2 * t);
export const ribbon = () => {
  const r = rng(77), W = 612;
  const strands = [[150, 30, 0.3, 9, 0.55], [120, 22, 2.1, 6, 0.4], [172, 16, 4.0, 5, 0.3], [92, 18, 5.2, 3.5, 0.22]];
  const out = [];
  strands.forEach(([y, amp, ph, th, alpha], k) => {
    const N = 28, xs = Array.from({ length: N }, (_, i) => 0.6 + (i / (N - 1)) * 610.8);
    const half = (x, i) => { const t = i / (N - 1), env = Math.pow(Math.sin(Math.PI * t), 0.7) * (0.55 + 0.45 * Math.sin(x / 60 + k * 2)); return (th / 2) * Math.max(0.1, env); };
    const up = xs.map((x, i) => [x, strandY(y, amp, ph, x) - half(x, i)]), dn = xs.map((x, i) => [x, strandY(y, amp, ph, x) + half(x, i)]).reverse();
    out.push({ ops: spline([...up, ...dn], true), fill: k < 2 ? "mid" : "light", alpha });
  });
  // rays: three alpha groups so tall rays are fainter (fade upward)
  const groups = [[], [], []];
  strands.slice(0, 3).forEach(([y, amp, ph], k) => {
    let x = 4 + r() * 10;
    while (x < 606) {
      const run = r() < 0.3 ? 2 + Math.floor(r() * 3) : 1; // occasional clumps
      for (let j = 0; j < run && x < 606; j++, x += 3 + r() * 4) {
        const len = 22 + Math.pow(r(), 1.5) * 70, base = strandY(y, amp, ph, x), wd = 1.1 + r() * 1.7, lean = (r() - 0.5) * 5;
        const g = len < 40 ? 0 : len < 62 ? 1 : 2;
        groups[g].push(["M", x - wd / 2, base], ["L", x + lean, base - len], ["L", x + wd / 2, base], ["Z"]);
      }
      x += 8 + r() * 26;
    }
  });
  groups.forEach((ops, g) => out.push({ ops, fill: "mid", alpha: [0.5, 0.36, 0.24][g] }));
  // sparse stipple of short ticks drifting above the rays
  const tick = [];
  for (let i = 0; i < 40; i++) { const x = r() * 600 + 6, y = 20 + r() * 70; tick.push(["M", x - 0.5, y + 2.5], ["L", x, y - 2.5], ["L", x + 0.5, y + 2.5], ["Z"]); }
  out.push({ ops: tick, fill: "light", alpha: 0.7 });
  return out;
};
export { xform, circleOps, oval, starPts, polar };
