// Regenerates crates/borealis-sheet/src/art.rs (and optional SVG/PNG previews) from the two art sets.
//   node tools/sheet-art/build.mjs [--svg <dir>]     (run from app/; output paths are relative to the app root)
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import * as colour from "./colour.mjs";
import * as ink from "./ink.mjs";
import { deathSave, hitDie } from "./glyphs.mjs";
import { bbox, opsToD, roundOps } from "./kit.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const appRoot = join(here, "../..");

// piece table: id (Rust variant), svg name, size in art units, nominal display scale (pt per unit)
export const PIECES = [
  ["FrameLetter", "frame-letter", 612, 792, 1, (s) => s.frame(612, 792, 121)],
  ["FrameA4", "frame-a4", 595, 842, 1, (s) => s.frame(595, 842, 131)],
  ["Corner", "corner", 124, 124, 1, (s) => s.corner()],
  ["Divider", "divider", 400, 20, 1, (s) => s.divider()],
  ["HeaderRule", "header-rule", 600, 12, 1, (s) => s.headerRule()],
  ...["str", "dex", "con", "int", "wis", "cha"].map((k) => [`Sigil${k[0].toUpperCase()}${k.slice(1)}`, `sigil-${k}`, 100, 100, 0.4, (s) => s.sigils[k]()]),
  ["BorealisRibbon", "borealis-ribbon", 612, 220, 1, (s) => s.ribbon()],
  ["HitDie", "hit-die", 24, 24, 0.75, (s, ornate) => hitDie(ornate)],
  ["DeathSave", "death-save", 24, 24, 0.75, (s, ornate) => deathSave(ornate)],
];
const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
export const SETS = {
  Colour: { mod: colour, ornate: false, tones: { ink: "#1F272B", gold: "#B38000", wash: "#305754", mid: "#4E585D", light: "#CFC9BC" } },
  Ink: { mod: ink, ornate: true, tones: { ink: "#000000", gold: "#000000", wash: "#555555", mid: "#595959", light: "#8C8C8C" } },
};

const clampOps = (ops, w, h) => ops.map((o) => [o[0], ...o.slice(1).map((v, i) => Math.min(i % 2 ? h : w, Math.max(0, v)))]);
const items = (set, piece) => {
  const its = piece[5](set.mod, set.ornate);
  return piece[0] === "BorealisRibbon" ? its.map((it) => ({ ...it, ops: clampOps(it.ops, piece[2], piece[3]) })) : its; // watermark bleeds: clip it here, not in the PDF
};
const rgbLit = (h) => `[${hex(h).join(", ")}]`;

function flat(ops) {
  const f = [];
  for (const o of roundOps(ops)) f.push({ M: 0, L: 1, C: 2, Z: 3 }[o[0]], ...o.slice(1));
  return f;
}
const num = (n) => (Number.isInteger(n) ? `${n}.0` : `${n}`);

export function svgOf(set, piece) {
  const [, , w, h] = piece, tones = SETS[set].tones;
  const body = items(SETS[set], piece).map((it) => {
    const a = it.alpha !== undefined && it.alpha !== 1 ? ` opacity="${it.alpha}"` : "";
    const d = opsToD(it.ops);
    return it.fill ? `<path d="${d}" fill="${tones[it.fill]}"${a}/>` : `<path d="${d}" fill="none" stroke="${tones[it.stroke]}" stroke-width="${it.w}" stroke-linecap="round" stroke-linejoin="round"${a}/>`;
  }).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}">${body}</svg>`;
}

function rust() {
  let data = "";
  const tables = [], golden = [];
  for (const [setName, set] of Object.entries(SETS)) {
    const names = [];
    let nPaths = 0, nNums = 0;
    for (const piece of PIECES) {
      const [id, , w, h] = piece, its = items(set, piece);
      for (const it of its) {
        const b = bbox(roundOps(it.ops)), m = it.w ? it.w / 2 : 0;
        if (b.x0 - m < -0.5 || b.y0 - m < -0.5 || b.x1 + m > w + 0.5 || b.y1 + m > h + 0.5) process.stderr.write(`warn: ${setName}/${id} item exceeds viewBox ${JSON.stringify(b)}\n`);
      }
      const cname = `${setName.toUpperCase()}_${id.replace(/([a-z])([A-Z])/g, "$1_$2").toUpperCase()}`;
      names.push(cname);
      nPaths += its.length; nNums += its.reduce((n, it) => n + flat(it.ops).length, 0);
      data += `#[rustfmt::skip]\nstatic ${cname}: [ArtPath; ${its.length}] = [\n`;
      for (const it of its) {
        const t = set.tones, fill = it.fill ? `Some(${rgbLit(t[it.fill])})` : "None", stroke = it.stroke ? `Some(Stroke { rgb: ${rgbLit(t[it.stroke])}, width: ${num(it.w)} })` : "None";
        data += `    ArtPath { fill: ${fill}, stroke: ${stroke}, alpha: ${num(it.alpha ?? 1)}, data: &[${flat(it.ops).map(num).join(",")}] },\n`;
      }
      data += "];\n";
    }
    golden.push(`(${nPaths}, ${nNums})`);
    tables.push(`#[rustfmt::skip]\nstatic ${setName.toUpperCase()}_PIECES: [ArtPiece; ${PIECES.length}] = [\n${PIECES.map((p, i) => `    ArtPiece { width: ${num(p[2])}, height: ${num(p[3])}, nominal_scale: ${num(p[4])}, paths: &${names[i]} },`).join("\n")}\n];\n`);
  }
  const idEnum = PIECES.map((p) => `    ${p[0]},`).join("\n");
  const all = PIECES.map((p) => `        ArtId::${p[0]},`).join("\n");
  const tmpl = readFileSync(join(here, "art.rs.tmpl"), "utf8");
  return tmpl.replace("//@@IDS@@", idEnum).replace("//@@ALL@@", all).replace("//@@COUNT@@", String(PIECES.length)).replace("//@@GOLDEN@@", `[${golden.join(", ")}]`).replace("//@@DATA@@", data + tables.join(""));
}

const main = process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);
const svgDir = process.argv.includes("--svg") ? process.argv[process.argv.indexOf("--svg") + 1] : null;
if (main && !process.argv.includes("--no-rust")) {
  const out = join(appRoot, "crates/borealis-sheet/src/art.rs");
  writeFileSync(out, rust());
  execFileSync("rustfmt", ["--edition", "2021", out]); // data statics are #[rustfmt::skip]
  process.stdout.write("wrote crates/borealis-sheet/src/art.rs\n");
}
if (main && svgDir) {
  mkdirSync(svgDir, { recursive: true });
  for (const set of Object.keys(SETS)) for (const p of PIECES) writeFileSync(join(svgDir, `${set.toLowerCase()}-${p[1]}.svg`), svgOf(set, p) + "\n");
  process.stdout.write(`wrote svgs to ${svgDir}\n`);
}
