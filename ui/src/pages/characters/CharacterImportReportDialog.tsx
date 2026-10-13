import type { ImportReport, MissingContentItem } from "../../api/contract";
import { Alert, Button, Expander, Modal } from "../../components";

/** One file of an import batch: its report, or why it failed. */
export interface CharacterImportEntry {
  fileName: string;
  report?: ImportReport;
  error?: string;
}

/** Missing items grouped by source book, then by element type. */
export function groupMissing(items: MissingContentItem[]): [string, [string, MissingContentItem[]][]][] {
  const bySource = new Map<string, Map<string, MissingContentItem[]>>();
  for (const m of items) {
    const source = m.source || "Unknown source";
    const type = m.type || "Other";
    const types = bySource.get(source) ?? new Map<string, MissingContentItem[]>();
    types.set(type, [...(types.get(type) ?? []), m]);
    bySource.set(source, types);
  }
  return [...bySource.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([s, t]) => [s, [...t.entries()].sort(([a], [b]) => a.localeCompare(b))]);
}

function EntrySection({ entry }: { entry: CharacterImportEntry }) {
  const r = entry.report;
  if (!r) {
    return (
      <Expander title={entry.fileName}>
        <Alert tone="danger">Could not import this file: {entry.error}</Alert>
      </Expander>
    );
  }
  const groups = groupMissing(r.missingContent);
  const clean = r.missingContent.length === 0 && r.unmappedSlots.length === 0;
  return (
    <Expander title={r.summary.name} headerExtra={<span className="v4-rep-state">{clean ? "Imported" : "Needs content"}</span>}>
      <div className="v4-rep">
        <p className="v4-rep-line">
          Imported <strong>{r.summary.name}</strong>{r.selectionsImported > 0 ? ` with ${r.selectionsImported} selections` : ""}.
        </p>
        {r.missingContent.length > 0 && (
          <section aria-label={`Missing content for ${r.summary.name}`}>
            <h3>Missing content</h3>
            <p className="v4-rep-hint">This character uses content that is not installed. Install it and the character picks it up, with no re-import.</p>
            {groups.map(([source, types]) => (
              <div key={source} className="v4-rep-group">
                <h4>{source}</h4>
                {types.map(([type, list]) => (
                  <div key={type}>
                    <span className="v4-rep-type">{type}</span>
                    <ul>{list.map((m) => <li key={m.id}>{m.name || m.id} <span className="v4-rep-id">{m.id}</span></li>)}</ul>
                  </div>
                ))}
              </div>
            ))}
          </section>
        )}
        {r.unmappedSlots.length > 0 && (
          <section aria-label={`Unmapped selections for ${r.summary.name}`}>
            <h3>Choices that could not be placed</h3>
            <ul>{r.unmappedSlots.map((u, i) => <li key={`${u.owner}-${u.name}-${i}`}>{u.type ? `${u.type}: ` : ""}{u.name || u.chosen} <span className="v4-rep-id">({u.reason})</span></li>)}</ul>
          </section>
        )}
        {r.invalidSelections.length > 0 && (
          <section aria-label={`Choices the rules reject for ${r.summary.name}`}>
            <h3>Choices the current rules reject</h3>
            <ul>{r.invalidSelections.map((d) => <li key={d}>{d}</li>)}</ul>
          </section>
        )}
        {r.divergences.length > 0 && <section><h3>Rules that changed</h3><ul>{r.divergences.map((d) => <li key={d}>{d}</li>)}</ul></section>}
        {r.warnings.length > 0 && <section><h3>Warnings</h3><ul>{r.warnings.map((d) => <li key={d}>{d}</li>)}</ul></section>}
      </div>
    </Expander>
  );
}

/** The import report for one or several characters (expanders, uppercase headings). */
export function CharacterImportReportDialog({ entries, onClose, onOpenContent, onRecheck, rechecking }: {
  entries: CharacterImportEntry[];
  onClose: () => void;
  /** Go to Additional Content to install what the characters need. */
  onOpenContent: () => void;
  onRecheck: () => void;
  rechecking?: boolean;
}) {
  const ok = entries.filter((e) => e.report).length;
  const failed = entries.length - ok;
  const needsContent = entries.some((e) => (e.report?.missingContent.length ?? 0) > 0 || (e.report?.unmappedSlots.length ?? 0) > 0);
  return (
    <Modal title="Import report" wide className="aur-rep-dlg" onClose={onClose} footer={
      <>
        {needsContent && <Button onClick={onOpenContent}>Import the content it needs</Button>}
        {needsContent && <Button disabled={rechecking} onClick={onRecheck}>Re-check missing content</Button>}
        <Button variant="primary" onClick={onClose}>Done</Button>
      </>
    }>
      <div className="stack">
        <Alert tone={failed > 0 ? "danger" : needsContent ? "warning" : "success"}>
          {entries.length === 1 ? (ok === 1 ? (needsContent ? "Imported, with missing content." : "Imported.") : "Nothing was imported.")
            : `${ok} of ${entries.length} characters imported${failed > 0 ? `, ${failed} failed` : ""}${needsContent ? ", some with missing content" : ""}.`}
        </Alert>
        {entries.map((e, i) => <EntrySection key={`${e.fileName}-${i}`} entry={e} />)}
      </div>
    </Modal>
  );
}
