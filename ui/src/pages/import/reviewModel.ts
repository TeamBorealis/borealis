import type { ImportElement, ImportReviewState } from "../../api/contract";

export type ReviewFilter = "all" | ImportReviewState;

export const STATE_LABEL: Record<ImportReviewState, string> = { ok: "OK", check: "Check this", not_imported: "Not imported" };
export const FILTER_LABEL: Record<ReviewFilter, string> = { all: "All", check: "Check this", not_imported: "Not imported", ok: "OK" };
export const FILTER_ORDER: ReviewFilter[] = ["all", "check", "not_imported", "ok"];

const RANK: Record<ImportReviewState, number> = { check: 0, not_imported: 1, ok: 2 };

export interface ReviewTotals { all: number; ok: number; check: number; not_imported: number; textOnly: number }

export function totalsOf(elements: readonly ImportElement[]): ReviewTotals {
  const n = (s: ImportReviewState) => elements.filter((e) => e.review.state === s).length;
  return { all: elements.length, ok: n("ok"), check: n("check"), not_imported: n("not_imported"), textOnly: elements.filter((e) => e.review.textOnly).length };
}

/** Flagged elements first (Check this, then Not imported), source order kept inside each group. */
export function visibleElements(elements: readonly ImportElement[], filter: ReviewFilter): ImportElement[] {
  const shown = filter === "all" ? [...elements] : elements.filter((e) => e.review.state === filter);
  return shown.map((e, i) => ({ e, i })).sort((a, b) => RANK[a.e.review.state] - RANK[b.e.review.state] || a.i - b.i).map((x) => x.e);
}

export const isFlagged = (e: ImportElement) => e.review.state !== "ok";

/** Every line of an element's JSON description, or its source excerpt, for the plain summary. */
export function summaryText(e: ImportElement): string {
  const d = e.json.description;
  return typeof d === "string" && d.trim() ? d : e.sourceExcerpt;
}
