// Small play-field glyphs (24 x 24): a hit-die and a death-save skull. `ornate` adds ink-mode facets and crossed bones.
import { disc, line, seg, strokes, star } from "./kit.mjs";

const hexPts = (r, cx = 12, cy = 12) => Array.from({ length: 6 }, (_, i) => [cx + Math.cos(Math.PI / 6 + (i * Math.PI) / 3) * r, cy + Math.sin(Math.PI / 6 + (i * Math.PI) / 3) * r]);

export const hitDie = (ornate) => {
  const h = hexPts(10.5), t = [0, 2, 4].map((i) => hexPts(6.2)[i]);
  const items = [seg(h, 1.1, "ink", 1, true), seg(t, 0.8, "ink", 1, true)];
  if (ornate) items.push(strokes(h.map((p, i) => [p, hexPts(6.2)[i]]), 0.6, "ink"), disc(12, 12, 1, "ink"));
  else items.push(disc(12, 12, 1.4, "gold"));
  return items;
};

export const deathSave = (ornate) => {
  const skull = line([[12, 2.5], [18, 5], [20, 10.5], [17.2, 15], [16.4, 18.2], [12, 19.4], [7.6, 18.2], [6.8, 15], [4, 10.5], [6, 5]], 1.1, "ink", 301, 0, 1, true);
  const items = [skull, disc(8.6, 10.4, 2.3, "ink"), disc(15.4, 10.4, 2.3, "ink"),
    seg([[12, 12.6], [10.9, 14.8], [13.1, 14.8]], 0.8, "ink", 1, true),
    strokes([[[9.4, 17], [9.4, 19.2]], [[12, 17.4], [12, 19.4]], [[14.6, 17], [14.6, 19.2]]], 0.7, "ink")];
  if (ornate) items.unshift(strokes([[[2.5, 21.5], [21.5, 3]], [[2.5, 3], [21.5, 21.5]]], 0.9, "ink", 0.7), disc(2.5, 21.5, 1.3, "ink"), disc(21.5, 3, 1.3, "ink"), disc(2.5, 3, 1.3, "ink"), disc(21.5, 21.5, 1.3, "ink"));
  else items.push(star(12, 22.4, 1.4, 0.5, "gold"));
  return items;
};
