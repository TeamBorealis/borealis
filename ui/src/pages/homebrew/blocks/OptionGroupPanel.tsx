import { useId, useState } from "react";
import type { HomebrewExprValue, HomebrewTagFilter } from "../../../api/contract";
import { Button, TextBox } from "../../../components";
import { useApi } from "../../../state/ApiContext";
import { setStatus } from "../../../state/status";
import { useAsync } from "../../../state/useAsync";
import { refusalText } from "../BulkActions";
import { useCreatorEnv } from "../env";
import type { Fields } from "./values";

/** The element type of class options (spec 4.8). */
export const OPTION_TYPE = "Option";
/** The creator form of a class option. */
const OPTION_KIND = "classOption";
/** Options of one group a class offers (Fighting Styles, Invocations): one page holds them. */
const OPTION_PAGE = 200;

/** The one tag a choice's "From" filter names (`Fighting Style (Knight)`), or null for a filter of another shape. */
export function groupOf(supports: unknown): string | null {
  const value = (supports as HomebrewExprValue<HomebrewTagFilter> | undefined)?.value;
  if (!value || value.shape !== "model") return null;
  let node = value.model.root;
  if (node.node === "group" && node.items.length === 1 && node.mode === "any") node = node.items[0]!;
  return node.node === "row" && node.row.row === "tag" ? node.row.tag : null;
}

/** The "From" filter of one tag. */
export function groupFilter(tag: string): HomebrewExprValue<HomebrewTagFilter> {
  return { value: { shape: "model", model: { specific: false, root: { node: "row", row: { row: "tag", tag } } } } };
}

/** An option made in the panel: saved to the pack at once. */
export interface MadeOption {
  id: string;
  name: string;
}

/**
 * A class's "Lets the player choose" a class option (spec 4.8), in one flow: the group (the tag its options carry, which
 * the choice offers), the options of that group already loaded or saved, and a new option by name. A new option is saved
 * to the pack at once (group, the class's name and edition): open it under Class options to give it rules. `onMade`
 * tells the dialog, which deletes them again when it is cancelled (P3).
 */
export function OptionGroupPanel({ fields, onFields, onMade }: { fields: Fields; onFields: (next: Fields) => void; onMade?: (option: MadeOption) => void }) {
  const api = useApi();
  const env = useCreatorEnv();
  const groupId = useId();
  const nameId = useId();
  const group = groupOf(fields.supports);
  const [groupText, setGroupText] = useState(group ?? "");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [made, setMade] = useState(0);
  const options = useAsync(
    () => (group
      ? api.homebrewReferenceOptions({
        target: { types: [OPTION_TYPE], tags: [group] }, query: "", filters: {}, page: { offset: 0, limit: OPTION_PAGE },
        ...(env.draftId ? { draftId: env.draftId } : {}),
      }).then((p) => p.items)
      : Promise.resolve([])),
    [api, group, env.draftId, made],
  );
  const kept = fields.supports !== undefined && group === null;

  const setGroup = (text: string) => {
    setGroupText(text);
    const tag = text.trim();
    const { supports: _s, ...rest } = fields;
    // the label follows the group while it is empty or still the group's (the choice's persistence key, spec 4.8)
    const label = typeof fields.name === "string" ? fields.name : "";
    const followName = label.trim() === "" || label === (group ?? "");
    onFields({ ...rest, ...(tag ? { supports: groupFilter(tag) } : {}), ...(followName ? { name: tag } : {}) });
  };

  const className = env.form?.kind === "class" && typeof env.form.fields.name === "string" ? env.form.fields.name.trim() : "";
  const add = async () => {
    const n = name.trim();
    if (!n || !group || !env.packId) return;
    setBusy(true);
    setError(null);
    let draftId: string | null = null;
    try {
      const fresh = await api.homebrewNewDraft({ packId: env.packId, kind: OPTION_KIND });
      draftId = fresh.draftId;
      const next = {
        ...fresh.form.fields, name: n, group,
        ...(className ? { className } : {}),
        ...(env.edition ? { edition: env.edition } : {}),
      };
      await api.homebrewUpdateDraft({ draftId, form: { ...fresh.form, fields: next } });
      const saved = await api.homebrewSaveDraft({ draftId });
      draftId = null;
      const id = saved.elementIds[0];
      if (id) onMade?.({ id, name: n });
      setName("");
      setMade((m) => m + 1);
      env.onPackChanged?.();
      setStatus(`Saved the option ${n} to the pack. Open it under Class options to give it rules.`);
    } catch (e) {
      setError(refusalText(e));
      if (draftId) await api.homebrewDiscardDraft({ draftId }).catch(() => undefined);
    } finally {
      setBusy(false);
    }
  };

  const rows = options.data ?? [];
  return (
    <section className="hb-option-group" aria-label="Class options of this choice">
      <h4 className="hb-section-title">Class options</h4>
      {kept
        ? <p className="hb-field-help">This choice offers options by a filter of several parts: edit it in From.</p>
        : (
          <div className="mg-f hb-field" data-field="optionGroup">
            <label htmlFor={groupId}>Group</label>
            <TextBox id={groupId} value={groupText} onChange={(e) => setGroup(e.target.value)} />
            <p className="hb-field-help">The tag every option of this choice carries, such as Fighting Style (Knight).</p>
          </div>
        )}
      {group && (
        <>
          {rows.length === 0
            ? <p className="hb-field-help">No option of this group yet.</p>
            : (
              <ul className="hb-ref-list" aria-label={`Options of ${group}`}>
                {rows.map((r) => <li key={r.id} className="hb-chip-item"><span className="hb-ref-name">{r.name}</span></li>)}
              </ul>
            )}
          <div className="mg-f hb-field" data-field="newOption">
            <label htmlFor={nameId}>New option</label>
            <span className="hb-row-controls">
              <TextBox id={nameId} value={name} onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); void add(); } }} />
              <Button onClick={() => void add()} disabled={busy || !name.trim() || !env.packId}>Add option</Button>
            </span>
            <p className="hb-field-help">Saved to the pack in this group. Open it under Class options to give it rules.</p>
          </div>
          {error && <p className="hb-error-line" role="alert">{error}</p>}
        </>
      )}
    </section>
  );
}
