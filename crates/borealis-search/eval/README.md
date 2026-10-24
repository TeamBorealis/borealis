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
- `baseline.json` is produced later by the harness; it does not exist yet.
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
was run or read before the labels were fixed. The next phase pools the union of the top 20 of every variant and
agent B judges unlabelled pairs without seeing which variant produced them.

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

1315 judgements over 203 queries; every query has at least one grade 3 or grade 2 document. Grade histogram:
grade 3 = 593, grade 2 = 288, grade 1 = 413, grade 0 = 21. Unlabelled pairs are unjudged and are filled in by
the pooling step.
