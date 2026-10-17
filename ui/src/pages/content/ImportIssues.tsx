import type { MigrationIssue } from "../../api/contract";
import { Expander } from "../../components/controls";

const SEVERITIES = [["error", "Errors"], ["warning", "Warnings"], ["info", "Notes"]] as const;
/** Rows listed per severity; an install report can hold tens of thousands of issues. */
export const MAX_LISTED_ISSUES = 500;
/** Group heading for issues not tied to one file (a whole-set finding). */
export const NO_FILE = "Whole import";

/** "Spell ID_PHB_FIREBALL, line 12 · element_dropped": where an issue is and its code. */
export function issueWhere(d: MigrationIssue): string {
  const element = [d.elementType, d.element].filter(Boolean).join(" ");
  const place = [element, d.line != null ? `line ${d.line}` : ""].filter(Boolean).join(", ");
  return [place, d.code].filter(Boolean).join(" · ");
}

/** Issues by source file (path inside the imported set), files in path order, each by line. */
export function groupByFile(list: readonly MigrationIssue[]): [string, MigrationIssue[]][] {
  const groups = new Map<string, MigrationIssue[]>();
  for (const d of list) {
    const key = d.file ?? NO_FILE;
    groups.set(key, [...(groups.get(key) ?? []), d]);
  }
  const byLine = (a: MigrationIssue, b: MigrationIssue) => (a.line ?? 0) - (b.line ?? 0);
  return [...groups].map(([file, rows]): [string, MigrationIssue[]] => [file, [...rows].sort(byLine)])
    .sort(([a], [b]) => (a === NO_FILE ? -1 : b === NO_FILE ? 1 : a.localeCompare(b)));
}

function SeverityList({ list, label }: { list: MigrationIssue[]; label: string }) {
  const shown = list.slice(0, MAX_LISTED_ISSUES);
  return (
    <Expander title={`${label} (${list.length})`} defaultOpen={false}>
      <ul className="v4-impdiag" aria-label={label}>
        {groupByFile(shown).map(([file, rows]) => [
          <li key={`f:${file}`} aria-label={`File ${file}`}><strong>{file}</strong><span className="muted small"> ({rows.length})</span></li>,
          ...rows.map((d, n) => (
            <li key={`${file}:${n}`}>{d.message}{issueWhere(d) && <span className="muted small"> ({issueWhere(d)})</span>}</li>
          )),
        ])}
      </ul>
      {list.length > shown.length && <p className="muted small">Only the first {MAX_LISTED_ISSUES} of {list.length} are listed.</p>}
    </Expander>
  );
}

/** A conversion report's issues: one expander per severity, grouped by source file, each with its element, line and code. */
export function ImportIssues({ issues }: { issues: readonly MigrationIssue[] }) {
  return (
    <>
      {SEVERITIES.map(([s, label]) => {
        const rows = issues.filter((d) => d.severity === s);
        return rows.length ? <SeverityList key={s} list={rows} label={label} /> : null;
      })}
    </>
  );
}
