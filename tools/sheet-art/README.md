# sheet-art

Generator for `crates/borealis-sheet/src/art.rs`: vector path tables for the Borealis character sheet
(frame, corners, header rule, divider, six ability sigils, hit-die and death-save glyphs, borealis ribbon),
in two sets: **Colour** (print palette) and **Ink** (black and greys only, richer filigree, rune ticks,
constellations and hatching, for the monochrome export mode).

The geometry core in `geom.mjs` is derived from anidoodle (Apache-2.0, (c) 2026 Alex Greenshpun; modified, see the header and `LICENSE-anidoodle`). No npm install needed.

```sh
# from app/
node tools/sheet-art/build.mjs                      # rewrites crates/borealis-sheet/src/art.rs (runs rustfmt)
node tools/sheet-art/build.mjs --svg /tmp/art-svg   # also writes one SVG per set and piece
node tools/sheet-art/preview.mjs <out-dir>          # PNG contact sheets (needs playwright-core from app/e2e)
```

Output is deterministic (seeded). After changing the art, update `GOLDEN_COUNTS` is automatic (generated); review the previews.

Files: `colour.mjs`, `ink.mjs` (the two designs), `glyphs.mjs` (small glyphs, `ornate` flag), `kit.mjs` (primitives),
`art.rs.tmpl` (Rust API and tests), `build.mjs` (emit), `preview.mjs`.
Print rules: strokes >= 0.3 pt at each piece's `nominal_scale` (sigils 0.4 pt/unit = 40 pt wide, glyphs 0.75).
