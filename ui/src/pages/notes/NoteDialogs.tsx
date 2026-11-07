import { useMemo, useState, type FormEvent } from "react";
import type { VaultEntry, VaultNote, VaultRenamePlan } from "../../api/contract";
import { Alert, Button, Modal, SearchInput } from "../../components";
import { baseName, displayName } from "./noteNames";
import { lf } from "./useNoteEditor";

/** The note changed on disk while the user had edits: keep mine, take theirs, or look at both first. Nothing is written yet. */
export function ConflictDialog({ name, mine, theirs, onKeepMine, onTakeTheirs, onLater }: {
  name: string; mine: string; theirs: VaultNote; onKeepMine: () => void; onTakeTheirs: () => void; onLater: () => void;
}) {
  const [both, setBoth] = useState(false);
  return (
    <Modal title="Note changed in another app" onClose={onLater} wide={both}
      footer={<>
        <Button onClick={() => setBoth((b) => !b)} aria-pressed={both}>{both ? "Hide both" : "Show both"}</Button>
        <span className="spacer" />
        <Button onClick={onTakeTheirs}>Take theirs</Button>
        <Button variant="primary" data-autofocus onClick={onKeepMine}>Keep mine</Button>
      </>}>
      <p>{name} was changed in another app (Obsidian or a sync service) while you were editing it. Nothing has been saved yet.</p>
      <p>Keep mine replaces the file with your version. Take theirs drops your edits and shows the file as it is now.</p>
      {both && (
        <div className="nt-compare">
          <label>Your version<textarea readOnly value={mine} aria-label="Your version" /></label>
          <label>In the file now<textarea readOnly value={lf(theirs.text)} aria-label="In the file now" /></label>
        </div>
      )}
    </Modal>
  );
}

/** The note was deleted on disk while the user had edits. */
export function MissingDialog({ name, onRecreate, onDiscard }: { name: string; onRecreate: () => void; onDiscard: () => void }) {
  return (
    <Modal title="Note deleted in another app" onClose={onDiscard}
      footer={<>
        <Button onClick={onDiscard}>Close note</Button>
        <Button variant="primary" data-autofocus onClick={onRecreate}>Save it again</Button>
      </>}>
      <p>{name} was deleted or moved in another app. Your edits are still here: save them as the note again, or close it.</p>
    </Modal>
  );
}

/** A name for a new or renamed note. */
export function NameDialog({ title, label, initial, confirm, error, onSubmit, onClose }: {
  title: string; label: string; initial: string; confirm: string; error: string | null; onSubmit: (name: string) => void; onClose: () => void;
}) {
  const [value, setValue] = useState(initial);
  const submit = (e: FormEvent) => { e.preventDefault(); if (value.trim()) onSubmit(value.trim()); };
  return (
    <Modal title={title} onClose={onClose}
      footer={<>
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="primary" type="submit" form="nt-name-form" disabled={!value.trim()}>{confirm}</Button>
      </>}>
      <form id="nt-name-form" onSubmit={submit}>
        <label className="field-label" htmlFor="nt-name">{label}</label>
        <input id="nt-name" className="input" data-autofocus value={value} onChange={(e) => setValue(e.target.value)} autoComplete="off" spellCheck={false} />
      </form>
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
    </Modal>
  );
}

/** Before a rename: every link that would change, line by line. Links change only with the user's OK. */
export function RenameLinksDialog({ plan, onRename, onClose }: { plan: VaultRenamePlan; onRename: (updateLinks: boolean) => void; onClose: () => void }) {
  const count = plan.files.reduce((n, f) => n + f.changes.length, 0);
  return (
    <Modal title="Update links?" onClose={onClose} wide
      footer={<>
        <Button onClick={onClose}>Cancel</Button>
        <span className="spacer" />
        <Button onClick={() => onRename(false)}>Rename only</Button>
        <Button variant="primary" data-autofocus onClick={() => onRename(true)}>Rename and update links</Button>
      </>}>
      <p>
        {displayName(baseName(plan.from))} becomes {displayName(baseName(plan.to))}. {count === 1 ? "1 line" : `${count} lines`} in{" "}
        {plan.files.length === 1 ? "1 note" : `${plan.files.length} notes`} link to it. Only the links change; nothing else in those notes.
      </p>
      <div className="nt-changes">
        {plan.files.map((f) => (
          <section key={f.path}>
            <h4>{displayName(f.path)}</h4>
            {f.changes.map((c) => (
              <div key={c.line} className="nt-change">
                <span className="nt-line">{c.line}</span>
                <del>{c.before}</del>
                <ins>{c.after}</ins>
              </div>
            ))}
          </section>
        ))}
      </div>
    </Modal>
  );
}

/** Pick a vault file to link to (the link is written the way `.obsidian/app.json` says). */
export function LinkPickerDialog({ entries, onPick, onClose }: { entries: readonly VaultEntry[]; onPick: (path: string) => void; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const files = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries.filter((e) => e.kind !== "folder" && (!q || e.path.toLowerCase().includes(q))).slice(0, 200);
  }, [entries, query]);
  return (
    <Modal title="Link to a note" onClose={onClose} footer={<Button onClick={onClose}>Cancel</Button>}>
      <SearchInput value={query} onChange={setQuery} label="Find a note" data-autofocus />
      <ul className="nt-pick" aria-label="Vault files">
        {files.map((e) => (
          <li key={e.path}><button type="button" className="nt-row" onClick={() => onPick(e.path)}>{displayName(e.path, e.kind)}</button></li>
        ))}
        {files.length === 0 && <li className="nt-empty">No file matches.</li>}
      </ul>
    </Modal>
  );
}
