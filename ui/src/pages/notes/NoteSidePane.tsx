import type { VaultBacklink, VaultTag } from "../../api/contract";
import { displayName } from "./noteNames";

/** The description pane of the notes view: the open note's backlinks, then the vault's tags. */
export function NoteSidePane({ title, backlinks, tags, onOpen, onTag }: {
  title: string; backlinks: VaultBacklink[] | null; tags: VaultTag[]; onOpen: (path: string, line: number | null) => void; onTag: (tag: string) => void;
}) {
  return (
    <aside className="desc-pane nt-side" aria-label="Note details">
      <div className="desc-scroll" tabIndex={0}>
        <h2 className="desc-title">{title}</h2>
        <h3>Backlinks</h3>
        {backlinks === null && <p className="nt-muted">Open a note to see the notes that link to it.</p>}
        {backlinks?.length === 0 && <p className="nt-muted">No other note links here.</p>}
        {backlinks && backlinks.length > 0 && (
          <ul className="nt-backlinks" aria-label="Backlinks">
            {backlinks.map((b) => (
              <li key={b.path}>
                <button type="button" className="nt-link" onClick={() => onOpen(b.path, null)}>{displayName(b.path)}</button>
                {b.count > 1 && <span className="nt-count">{b.count}</span>}
                {b.lines.map((l) => (
                  <button key={l.line} type="button" className="nt-snippet" onClick={() => onOpen(b.path, l.line)}>{l.text}</button>
                ))}
              </li>
            ))}
          </ul>
        )}
        <h3>Tags</h3>
        {tags.length === 0 && <p className="nt-muted">No tags in this vault.</p>}
        <ul className="nt-tags" aria-label="Tags">
          {tags.map((t) => (
            <li key={t.tag}>
              <button type="button" className="nt-chip tag" onClick={() => onTag(t.tag)}>#{t.tag}<span className="nt-count">{t.count}</span></button>
            </li>
          ))}
        </ul>
      </div>
    </aside>
  );
}
