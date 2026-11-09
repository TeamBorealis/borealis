import type { MissingContentItem, UnmappedSlotItem } from "../../api/contract";
import { Alert, Button, Expander, Modal } from "../../components";
import "./characters.css";

/** What to do about missing content: the same sentence wherever the list is shown. */
export const MISSING_CONTENT_HINT = "This character uses content that is not installed. Add the content these come from in Additional Content, "
  + "then re-check: the character picks it up, with no re-import.";

/** Missing items grouped by the source their ids name, then by element type. */
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

/** The missing content of one character: by source, then type, each item's name and the id the file names (GitHub #37). */
export function MissingContentList({ items }: { items: MissingContentItem[] }) {
  return (
    <>
      {groupMissing(items).map(([source, types]) => (
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
    </>
  );
}

/** Selections that could not be placed, as the import report lists them. */
export function UnmappedList({ items }: { items: UnmappedSlotItem[] }) {
  return <ul>{items.map((u, i) => <li key={`${u.owner}-${u.name}-${i}`}>{u.type ? `${u.type}: ` : ""}{u.name || u.chosen} <span className="v4-rep-id">({u.reason})</span></li>)}</ul>;
}

/** One character's outstanding content in a {@link MissingContentDialog}. */
export interface MissingContentEntry {
  name: string;
  missing: MissingContentItem[];
  unmapped?: UnmappedSlotItem[];
}

function EntryBody({ entry }: { entry: MissingContentEntry }) {
  const unmapped = entry.unmapped ?? [];
  return (
    <div className="v4-rep">
      {entry.missing.length > 0 && (
        <section aria-label={`Missing content for ${entry.name}`}>
          <h3>Missing content</h3>
          <MissingContentList items={entry.missing} />
        </section>
      )}
      {unmapped.length > 0 && (
        <section aria-label={`Unmapped selections for ${entry.name}`}>
          <h3>Choices that could not be placed</h3>
          <UnmappedList items={unmapped} />
        </section>
      )}
    </div>
  );
}

/**
 * The content one or several characters name that is not installed (GitHub #37): opened from the Build page's notice and after
 * a Re-check on the Characters page, so the list is never only a count.
 */
export function MissingContentDialog({ entries, onClose, onOpenContent, onRecheck, rechecking }: {
  entries: MissingContentEntry[];
  onClose: () => void;
  /** Go to Additional Content to install what the characters need. */
  onOpenContent: () => void;
  onRecheck?: () => void;
  rechecking?: boolean;
}) {
  const shown = entries.filter((e) => e.missing.length > 0 || (e.unmapped?.length ?? 0) > 0);
  const count = shown.reduce((n, e) => n + e.missing.length, 0);
  return (
    <Modal title="Missing content" wide className="aur-rep-dlg" onClose={onClose} footer={
      <>
        {shown.length > 0 && <Button onClick={onOpenContent}>Import the content it needs</Button>}
        {shown.length > 0 && onRecheck && <Button disabled={rechecking} onClick={onRecheck}>Re-check missing content</Button>}
        <Button variant="primary" onClick={onClose}>Done</Button>
      </>
    }>
      <div className="stack">
        {shown.length === 0 ? <Alert tone="success">Nothing is missing any more.</Alert> : (
          <>
            <Alert tone="warning">{count} content element{count === 1 ? " was" : "s were"} not found{shown.length > 1 ? ` for ${shown.length} characters` : ""}.</Alert>
            <p className="v4-rep-hint">{MISSING_CONTENT_HINT}</p>
          </>
        )}
        {shown.length === 1 ? <EntryBody entry={shown[0]!} />
          : shown.map((e, i) => <Expander key={`${e.name}-${i}`} title={e.name}><EntryBody entry={e} /></Expander>)}
      </div>
    </Modal>
  );
}
