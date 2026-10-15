import type { ContentIndex, UpdateIndexesResult } from "../../api/contract";

/** What the Indexes tab tells the user after an update: a toast and the status line under the buttons. */
export interface IndexUpdateText {
  toast: string;
  tone: "info" | "warning" | "error";
  status: string;
}

export const UP_TO_DATE = "Everything is up to date.";

/** "<name>: <problem>" for every index whose last check failed or was incomplete. */
const problemsOf = (indexes: readonly ContentIndex[]): string[] =>
  indexes.filter((i) => i.problem).map((i) => `${i.name}: ${i.problem}`);

/**
 * The outcome of `updateIndexes`, worded so a failure never reads as success: "up to date" only when
 * nothing failed and every index has been downloaded at least once.
 */
export function describeIndexUpdate(r: UpdateIndexesResult, indexes: readonly ContentIndex[], checkedText: string): IndexUpdateText {
  const notes = [
    r.changed.length ? `${r.changed.length} file(s) changed since they were first downloaded.` : "",
    r.newHosts.length ? `New download source(s): ${r.newHosts.join(", ")}.` : "",
  ].filter(Boolean).join(" ");
  const withNotes = (text: string) => (notes ? `${text} ${notes}` : text);
  const problems = problemsOf(indexes);
  const reason = problems.length ? ` ${problems.join(" ")}` : "";
  const failed = r.failed.length;
  if (failed && !r.updated) {
    return { tone: "error", toast: `Nothing was downloaded: ${failed} could not be updated.${reason}`, status: withNotes(`Nothing was downloaded: ${failed} could not be updated.${reason}`) };
  }
  if (failed) {
    const text = `${r.updated} content pack(s) downloaded and loaded; ${failed} could not be updated.${reason}`;
    return { tone: "warning", toast: text, status: withNotes(text) };
  }
  if (r.updated) {
    return { tone: notes ? "warning" : "info", toast: withNotes(`${r.updated} content pack(s) downloaded and loaded.`), status: withNotes(`${r.updated} content pack(s) downloaded and loaded.`) };
  }
  const never = indexes.filter((i) => !i.lastChecked);
  if (never.length) {
    const text = `Nothing was downloaded: ${never.map((i) => i.name).join(", ")} has not been downloaded yet.`;
    return { tone: "warning", toast: text, status: text };
  }
  return { tone: notes ? "warning" : "info", toast: withNotes(UP_TO_DATE), status: withNotes(checkedText) };
}
