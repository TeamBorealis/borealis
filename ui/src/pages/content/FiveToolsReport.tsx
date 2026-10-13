import type { FiveToolsImportReport, ImportNote } from "../../api/contract";
import { Alert } from "../../components";
import { Expander } from "../../components/controls";
import { groupNotes, isExpectedSkip } from "./fiveTools";

/** Examples listed per reason before the rest is summarised. */
const MAX_EXAMPLES = 50;

function NoteGroups({ title, notes }: { title: string; notes: ImportNote[] }) {
  if (notes.length === 0) return null;
  const groups = groupNotes(notes);
  return (
    <div className="stack" role="group" aria-label={title}>
      <h3>{title} ({notes.length})</h3>
      {groups.map((g) => (
        <Expander key={g.key} defaultOpen={false} title={<><span className="v4-5et-count">{g.notes.length}</span> {g.label}</>}>
          <ul className="v4-5et-notes">
            {g.notes.slice(0, MAX_EXAMPLES).map((n, i) => <li key={i}><strong>{n.element}</strong>{g.notes.length > 1 && <>: {n.reason}</>}</li>)}
            {g.notes.length > MAX_EXAMPLES && <li className="muted">and {g.notes.length - MAX_EXAMPLES} more</li>}
          </ul>
        </Expander>
      ))}
    </div>
  );
}

/** Import result: counts, replaced packs, notes grouped by reason; expected core skips collapse into one line. */
export function FiveToolsReport({ report }: { report: FiveToolsImportReport }) {
  const expected = report.skipped.filter(isExpectedSkip);
  const skipped = report.skipped.filter((n) => !isExpectedSkip(n));
  const replaced = report.replaced > 0 ? ` Replaced ${report.replaced} existing pack(s).` : "";
  return (
    <div className="stack" role="status">
      <Alert tone="success">{report.converted} elements converted into {report.packs.length} pack(s).{replaced} Marked "Local import — not redistributable".</Alert>
      <NoteGroups title="Partially converted" notes={report.partial} />
      <NoteGroups title="Skipped" notes={skipped} />
      {expected.length > 0 && <p className="muted small">{expected.length} entries already defined by the core rules (languages, skills, item types and similar) were not imported again.</p>}
    </div>
  );
}
