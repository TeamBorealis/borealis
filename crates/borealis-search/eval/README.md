# Search evaluation set (spec `local-search-spec.md` section 6, WP S1.3 phase A)

Our own queries and relevance judgements over the bundled SRD corpora only
(`content/srd-5.1*`, `content/srd-5.2.1*`, including the rules-text packs; CC-BY-4.0).
No 5etools, `corpus/` or `re-private/` content is used.

## Files

- `queries.jsonl`: `{id, text, category, ruleset?}`. `ruleset` (`"2014"` or `"2024"`) is present on the
  edition-filtered queries and on a few queries that name an edition in the text.
- `qrels.jsonl`: `{query_id, doc_key, grade}` with grade 3 = the answer / target element,
  2 = directly relevant section, 1 = related, 0 = judged irrelevant (used for documents of the other edition
  on edition-filtered queries, so a ruleset filter can be checked).
- `qrels-pool.jsonl`: same shape; the pooled judge's grades (protocol B below). The harness loads both files;
  on a conflict the labeller's grade wins.
- `baseline.json`: the adopted configuration and its nDCG@10 (overall and per category); the CI regression test
  `tests/regression.rs` reads it. Written with `--write-baseline`.
- `tests/eval_files.rs` validates all of the above (JSON shape, unique ids, every `query_id` and `doc_key`
  exists in the built corpora, category minimums, every query has a grade 2 or 3 document).

## doc_key scheme (as built by `corpus.rs`)

- Rules-text section: `rules:<pack>/<section-id>` with `<pack>` = `srd-5.1-rules-text` or `srd-5.2.1-rules-text`,
  for example `rules:srd-5.1-rules-text/combat/cover`.
- Typed element (Lore corpus, and Conditions / Rules Glossary Entries which are also in the Rules corpus): the
  element id, for example `ID_PHB_SPELL_FIREBALL` (2014) or `ID_2024_SPELL_FIREBALL` (2024).
- Sections or elements longer than the chunk limit are split: `<key>#1`, `#2`, ... The labels point to the first
  (primary) part, `#1`, of a split document.

## Protocol

Labelling is blind, as in HB1: labeller agent A wrote the queries and the judgements from the corpus text alone
(a throwaway script listed every `doc_key` with its title, heading path and text; relevant documents were found by
reading and plain text search, not by any ranker). No search, ranker, query code, synonym table or search result
was run or read before the labels were fixed.

Pooling (protocol B): the union of the top 20 of every variant (V0, V1, V2, V2b and the seven V2 ablations) minus the
labelled pairs, 5,701 pairs, was shuffled and judged by a separate blind judge that saw only query text, document
title and a 700-character excerpt (no variant, rank, ruleset field or edition tag). Its grades are in
`qrels-pool.jsonl`. The pool file itself (`pool.jsonl`, 3.5 MB) is not committed: `--pool` regenerates it
(deterministic, fixed seed; it now lists only pairs that are still unjudged, e.g. after a ranker change).

Both editions are covered. A query without `ruleset` treats the same element in both editions as the target
(both get grade 3); a query with `ruleset` marks the other edition's document grade 0 where it is the obvious
confusion.

## Counts per category

| Category | Queries | Spec minimum |
|---|---|---|
| exact name | 43 | 40 |
| typo | 32 | 30 |
| prefix/partial | 26 | 25 |
| paraphrase/descriptive | 31 | 30 |
| rules question | 38 | 35 |
| abbreviation/synonym/rename | 19 | 15 |
| edition-filtered (7 twin pairs, same text, 2014 vs 2024) | 14 | 10 |
| **SRD total** | **203** | **185** |

The **sessions** category (30 queries over the synthetic campaign fixture) is skipped on purpose: the session
recorder is not built yet. It is added with the recorder.

## Judgements

- Labeller A (`qrels.jsonl`): 1,315 judgements over 203 queries; grade 3 = 593, grade 2 = 288, grade 1 = 413,
  grade 0 = 21. Every query has at least one grade 3 or 2 document.
- Pooled judge B (`qrels-pool.jsonl`): 5,701 judgements, none overlapping A's; grade 3 = 26, grade 2 = 569,
  grade 1 = 1,898, grade 0 = 3,208.
- V4 pooling (S3.3): the unjudged top 20 of the eight V4 configurations (Q8_0 at 768 / 512 / 256 / 128 dims,
  Q6_K-te-Q4_K and Q5_K_M at 256 / 128) added 2,734 pairs, judged blind the same way (query text, title,
  700-character excerpt, no variant): grade 3 = 4, grade 2 = 52, grade 1 = 621, grade 0 = 2,057. Appended to
  `qrels-pool.jsonl`; `baseline.json` was rewritten for the larger judgement set (same ranker, nDCG@10 0.7404 ->
  0.7285: the new relevant documents raise the ideal DCG).
- Total 9,750. Unjudged pairs count as grade 0; with both files `judged@10` is 1.00 for every variant.

## Harness (phase B)

- `cargo run --release -p borealis-search --example eval -- --variant V2` prints the category x metric table
  (also `--variant all`, `--compare A B`, `--pool`, `--write-baseline V`). Variants: V0 (borealis-ai BM25), V1, V2,
  V2b (combined-field BM25F emulation), V2-t1..V2-t7 (V2 minus one tweak); V3 is the S1.4 hook (not adopted).
- V4 (S3.3) needs the model files: `--v4-cache <model.gguf>:<max_tokens>` embeds every chunk once into
  `BOREALIS_V4_CACHE`, `--v4 <model.gguf>:<max_tokens>:<dims>...` evaluates hybrid configurations against
  `T3,4,6,7` under the hybrid adoption rule (`BOREALIS_LATENCY_BUDGET=phone` for the mid-phone budget) and prints
  per-stage timings, `--v4-pool <out> <spec>...` writes their unjudged pairs. `BOREALIS_CONTENT_DIR` and
  `BOREALIS_EVAL_DIR` point the harness at copies pushed to a phone or emulator.
- `--pool` writes `pool.jsonl` (git-ignored): the union of every variant's top 20 per query minus judged pairs,
  shuffled with a fixed seed, no variant information.
- Variants can also be given as tweak sets: `T3,4,6,7` (exactly those tweaks, Tantivy fields), `B3,4,6,7` (same on
  the combined-field BM25F index). `--select` runs greedy forward selection from V1 under the adoption rule and
  `--sweep <mask>` sweeps the tweak 1 field weights.
- The adopted configuration is `T3,4,6,7` (prefix, typo, synonyms, edition); it is the crate default
  (`Tweaks::default()`). Results and p-values: `specs/search-eval-1.md`.
- `tests/regression.rs` fails when the config named in `baseline.json` drops more than 0.01 nDCG@10 below it (or a
  category more than 0.02).
