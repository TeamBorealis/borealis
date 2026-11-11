import { useState } from "react";
import type { ContentPack, HomebrewBulkPatch, HomebrewElementSummary } from "../../api/contract";
import { Alert, Button, ComboBox, Dialog, TextBox, useToast } from "../../components";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import { useMessageBox } from "./useMessageBox";

/** What a multi-select action did, for the workspace to follow (reload, close or reopen the open element). */
export interface BulkDone {
  action: "edit" | "move" | "duplicate" | "delete" | "undo";
  /** The chosen elements (gone from the pack after a move or a delete). */
  ids: string[];
}

type Asking = "edition" | "addTag" | "removeTag" | "move" | null;

/** The edition choices, as the GENERAL tab's Edition combo words them. */
const EDITIONS = [
  { value: "", label: "The pack's edition" },
  { value: "dnd2014", label: "2014" },
  { value: "dnd2024", label: "2024" },
  { value: "any", label: "Either edition" },
];

/** A refusal's sentences: the message, then each problem it lists (`details`, as a refused save sends them). */
export function refusalText(e: unknown): string {
  const details = (e as { details?: unknown } | null)?.details;
  const lines = Array.isArray(details) ? details.map((d) => String((d as { message?: unknown }).message ?? "")).filter(Boolean) : [];
  return [errorMessage(e), ...lines].join(" ");
}

const count = (n: number, one: string): string => `${n} ${one}${n === 1 ? "" : "s"}`;

export interface BulkActionsProps {
  packId: string;
  /** The chosen elements, in list order. */
  chosen: readonly HomebrewElementSummary[];
  /** The user's homebrew packs (Move to...). */
  packs: readonly ContentPack[];
  /** Called before a change: sends the open element's pending edits (a change to it must not be overwritten). */
  beforeChange: () => Promise<void>;
  onDone: (done: BulkDone) => void;
  onClear: () => void;
}

/**
 * Multi-select edits (spec 12.5): set the edition, add or remove a tag, move to another homebrew pack, duplicate, delete.
 * Each is one write and one undoable step: the toast's Undo puts every element back as it was.
 */
export function BulkActions({ packId, chosen, packs, beforeChange, onDone, onClear }: BulkActionsProps) {
  const api = useApi();
  const toast = useToast();
  const { ask, box } = useMessageBox();
  const [asking, setAsking] = useState<Asking>(null);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ids = chosen.map((r) => r.id);
  const others = packs.filter((p) => p.kind === "homebrew" && p.id !== packId);
  const what = chosen.length === 1 ? `“${chosen[0]!.name}”` : count(chosen.length, "element");

  const offerUndo = (text: string, undoId: string | null) => {
    setStatus(text);
    if (!undoId) { toast.push(text); return; }
    toast.push(text, "info", {
      label: "Undo",
      run: () => {
        void api.homebrewUndo({ undoId })
          .then(() => { setStatus("Undone."); onDone({ action: "undo", ids }); })
          .catch((e) => toast.push(errorMessage(e), "error"));
      },
    });
  };

  const run = async (patch: HomebrewBulkPatch, action: BulkDone["action"], done: (n: number) => string) => {
    setBusy(true);
    setError(null);
    try {
      await beforeChange();
      const r = await api.homebrewBulkEdit({ packId, elementIds: ids, patch });
      setAsking(null);
      setValue("");
      if (r.elementIds.length === 0) { setStatus("Nothing needed a change."); return; }
      offerUndo(done(r.elementIds.length), r.undoId);
      onDone({ action, ids });
    } catch (e) {
      if (asking) setError(refusalText(e));
      else toast.push(refusalText(e), "error");
    } finally {
      setBusy(false);
    }
  };

  /** Delete (spec 10.3): the scan of local characters first; characters that use them are named before anything goes. */
  const remove = async () => {
    try {
      await beforeChange();
      let r = await api.homebrewDeleteElements({ packId, elementIds: ids, confirm: false });
      if (!r.deleted) {
        const names = r.usedBy.map((c) => c.name).join(", ");
        const answer = await ask({
          title: "Delete", cancelId: "cancel",
          message: `${count(r.usedBy.length, "character")} use ${what}: ${names}. They will lose it. Delete anyway?`,
          buttons: [{ id: "delete", label: "Delete", variant: "danger" }, { id: "cancel", label: "Cancel" }],
        });
        if (answer !== "delete") return;
        r = await api.homebrewDeleteElements({ packId, elementIds: ids, confirm: true });
      }
      offerUndo(`Deleted ${what}.`, r.undoId);
      onDone({ action: "delete", ids });
    } catch (e) {
      toast.push(refusalText(e), "error");
    }
  };

  const open = (a: Asking) => { setAsking(a); setValue(a === "edition" ? "" : a === "move" ? others[0]?.id ?? "" : ""); setError(null); };
  const apply = () => {
    if (asking === "edition") void run({ edition: (value || null) as HomebrewBulkPatch["edition"] }, "edit", (n) => `Changed the edition of ${count(n, "element")}.`);
    if (asking === "addTag" && value.trim()) void run({ addTags: [value.trim()] }, "edit", (n) => `Added the tag ${value.trim()} to ${count(n, "element")}.`);
    if (asking === "removeTag" && value.trim()) void run({ removeTags: [value.trim()] }, "edit", (n) => `Removed the tag ${value.trim()} from ${count(n, "element")}.`);
    if (asking === "move" && value) {
      const to = others.find((p) => p.id === value)?.name ?? "the other pack";
      void run({ moveToPackId: value }, "move", (n) => `Moved ${count(n, "element")} to ${to}.`);
    }
  };
  const titles: Record<Exclude<Asking, null>, string> = { edition: "Set edition", addTag: "Add a tag", removeTag: "Remove a tag", move: "Move to another pack" };

  return (
    <div className="hb-bulk" role="toolbar" aria-label={`${what} chosen`}>
      <span className="hb-bulk-count">{count(chosen.length, "element")} chosen</span>
      <button type="button" className="hb-tool" disabled={busy} onClick={() => open("edition")}>Edition...</button>
      <button type="button" className="hb-tool" disabled={busy} onClick={() => open("addTag")}>Add tag...</button>
      <button type="button" className="hb-tool" disabled={busy} onClick={() => open("removeTag")}>Remove tag...</button>
      <button type="button" className="hb-tool" disabled={busy || others.length === 0} title={others.length === 0 ? "Make another homebrew pack first" : undefined} onClick={() => open("move")}>Move to...</button>
      <button type="button" className="hb-tool" disabled={busy} onClick={() => void run({ duplicate: true }, "duplicate", (n) => `Made ${n} ${n === 1 ? "copy" : "copies"}.`)}>Duplicate</button>
      <button type="button" className="hb-tool hb-danger" disabled={busy} onClick={() => void remove()}>Delete</button>
      <button type="button" className="hb-tool" onClick={onClear}>Clear</button>
      {asking && (
        <Dialog title={titles[asking]} onClose={() => setAsking(null)} className="hb-bulk-dialog"
          footer={<><Button onClick={() => setAsking(null)}>Cancel</Button><Button variant="primary" disabled={busy || (asking !== "edition" && !value.trim())} onClick={apply}>Apply</Button></>}>
          <p>{titles[asking]} for {what}. One step: Undo puts them back.</p>
          {asking === "edition" && <ComboBox aria-label="Edition" value={value} onChange={setValue} options={EDITIONS} />}
          {(asking === "addTag" || asking === "removeTag") && (
            <TextBox aria-label="Tag" value={value} autoFocus placeholder="Tag" onChange={(e) => setValue(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") apply(); }} />
          )}
          {asking === "move" && <ComboBox aria-label="Pack" value={value} onChange={setValue} options={others.map((p) => ({ value: p.id, label: p.name }))} />}
          {asking === "edition" && <p className="hb-field-help">A class keeps its edition. Other content of one edition in a pack of the other is adapted when a character takes it.</p>}
          {asking === "move" && <p className="hb-field-help">Characters keep what they have: the elements keep their ids and their edition.</p>}
          {error && <Alert tone="danger" role="alert">{error}</Alert>}
        </Dialog>
      )}
      {box}
    </div>
  );
}
