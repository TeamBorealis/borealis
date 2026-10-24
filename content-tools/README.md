# content-tools

Maintainer-only scripts that generated the bundled content packs in `content/`
(`srd-5.1`, `srd-5.1-monsters`, `srd-5.2.1`, `srd-5.2.1-monsters`, `srd-5.1-rules-text`, `srd-5.2.1-rules-text`). They are not needed to build or run
Borealis: the shipped packs are **generated output**, committed as JSON. Regenerating them is only
useful if you change how the packs are produced.

## External inputs

The scripts read the official SRD PDFs, which are not included here. Download them from the publisher:

- SRD 5.1 (CC-BY-4.0): <https://dnd.wizards.com/resources/systems-reference-document>
- SRD 5.2.1 (CC-BY-4.0): <https://www.dndbeyond.com/srd>

Place them (maintainer-local) as `input/srd/SRD_CC_v5.1.pdf` and `input/srd/SRD_CC_v5.2.1.pdf`, in the
directory two levels above `content-tools/` (the scripts compute this path from their own location).
`pdftotext`/`pdftohtml` (poppler) and Python 3 are required.

Optional: some scripts align element ids with an external set of id fixtures so that previously saved
characters keep resolving. Those fixtures are not part of this repository and are maintainer-local; point
`BOREALIS_ID_FIXTURES` at a directory of XML files to use them. Without them the scripts derive ids
themselves, and the committed packs remain valid.

## Layout

- `srd51/`, `srd521/`: rules packs (classes, species, spells, items, rules).
- `srd51-monsters/`, `srd521-monsters/`: typed monster stat blocks.
- `rules-text/`: the `srd-5.1-rules-text` and `srd-5.2.1-rules-text` search packs (`borealis-docs/1`, see
  `docs/rules-text-format.md`). `python3 -I content-tools/rules-text/build.py` rebuilds both byte-identically.
- `../crates/borealis-engine/tools/gen_core_pack.py`: generates `content/core`.

Intermediate files (`*.pkl`, raw text dumps) are scratch output of the extraction steps.

## Licence of the output

The generated packs contain material from the SRD under CC-BY-4.0, with modifications. Keep the
`ATTRIBUTION.md` in each pack and the root `NOTICE` intact when redistributing.
