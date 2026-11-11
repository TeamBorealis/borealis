import { type KeyboardEvent, useEffect, useMemo, useState } from "react";
import type { HomebrewPickerFilters, HomebrewPickerRow, HomebrewPickerTarget, HomebrewRef } from "../../../api/contract";
import { Button, ComboBox, Dialog, Expander, FocusBar, Icon } from "../../../components";
import { usePhone } from "../../../lib/phone";
import { editionsOf, effectiveFilter, NO_FILTER, type PickerFilter, sourcesOf, versionPasses } from "../../../lib/versions";
import { useApi } from "../../../state/ApiContext";
import { useAsync } from "../../../state/useAsync";
import { DescriptionPane } from "../../builder/DescriptionPane";
import { PickerFilterBar } from "../../builder/PickerFilterBar";
import { PickerRow } from "../../builder/PickerRow";
import "../../builder/builder.css";
import "../parts.css";
import { useCreatorEnv } from "../env";
import { isSpellTarget, refOf, sameId, shownRow, targetNoun, toVersion } from "./refRows";
import { SpellPickerGrid } from "./SpellPickerGrid";

/** Rows one picker query reads (spec 5.1 pages; a filter narrows on the backend, the bar's filters on screen). */
export const PICKER_PAGE = 300;
const QUERY_PAUSE_MS = 200;

/** The typed search, sent once typing pauses. */
function usePaused(text: string): string {
  const [paused, setPaused] = useState(text);
  useEffect(() => {
    const t = setTimeout(() => setPaused(text), QUERY_PAUSE_MS);
    return () => clearTimeout(t);
  }, [text]);
  return paused;
}

export interface ReferencePickerProps {
  /** The field's label ("Spell", "What it gives"): the expander title and the dialog title. */
  title: string;
  target: HomebrewPickerTarget;
  /** Ids chosen now. */
  chosen: readonly string[];
  onChoose: (picks: HomebrewRef[]) => void;
  onClose: () => void;
  /** Filters every query of this picker keeps ("Start from a template": SRD elements only). */
  filters?: HomebrewPickerFilters;
}

/** Up/Down move between rows and describe the row reached (the Build picker's keys). */
function onKeyNav(e: KeyboardEvent<HTMLButtonElement>): void {
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  e.preventDefault();
  const li = e.currentTarget.closest("li");
  const sib = e.key === "ArrowDown" ? li?.nextElementSibling : li?.previousElementSibling;
  const btn = sib?.querySelector<HTMLButtonElement>("button.pick-main");
  if (btn) { btn.focus(); btn.click(); }
}

/**
 * The reference picker dialog (spec 5.1, 13.5): the Build picker's expander band, focus bar, filter bar (search, Edition,
 * Source) and rows (NAME, italic summary, prerequisites, SOURCE), the chosen line with its X, and the description pane on
 * the right. Spells get the Magic grid and filter bar (13.7); `multiple` pickers keep a check per row. Double-click,
 * Enter or Choose picks. On phones it is a bottom sheet with Choose under the description (spec 16, O2 B).
 */
export function ReferencePicker({ title, target, chosen, onChoose, onClose, filters }: ReferencePickerProps) {
  const api = useApi();
  const env = useCreatorEnv();
  const phone = usePhone();
  const multiple = !!target.multiple;
  const [filter, setFilter] = useState<PickerFilter>(NO_FILTER);
  const [type, setType] = useState("");
  const [picked, setPicked] = useState<HomebrewRef[]>(chosen.map((id) => ({ id, name: "", type: "" })));
  const [described, setDescribed] = useState<HomebrewPickerRow | null>(null);
  const query = usePaused(filter.text);
  const types = type ? [type] : target.types;
  const data = useAsync(
    () => api.homebrewReferenceOptions({ target: { ...target, types }, query, filters: filters ?? {}, page: { offset: 0, limit: PICKER_PAGE }, ...(env.draftId ? { draftId: env.draftId } : {}) }),
    [api, JSON.stringify(target), JSON.stringify(filters ?? {}), type, query, env.draftId],  
  );
  const rows = useMemo(() => (data.data?.items ?? []).map(toVersion), [data.data]);
  const allRows = useMemo(() => rows.map((v) => shownRow(v, false)), [rows]);
  const editions = editionsOf(allRows);
  const shownFilter = effectiveFilter(filter, editions);
  const visible = rows.filter((v) => versionPasses(v, shownFilter));
  const typeChoices = useMemo(() => [...new Set((data.data?.items ?? []).map((r) => r.type))].sort(), [data.data]);
  const isPicked = (id: string) => picked.some((p) => sameId(p.id, id));
  const noun = targetNoun(target);

  const choose = (row: HomebrewPickerRow) => {
    if (!multiple) { onChoose([refOf(row)]); return; }
    setPicked((cur) => (cur.some((p) => sameId(p.id, row.id)) ? cur.filter((p) => !sameId(p.id, row.id)) : [...cur, refOf(row)]));
  };
  /** A pick carried in by id only gets its name once its row is loaded. */
  const named = (p: HomebrewRef): HomebrewRef => {
    if (p.name) return p;
    const v = rows.find((x) => sameId(x.id, p.id));
    return v ? refOf(v.row) : p;
  };
  const done = () => {
    if (multiple) { onChoose(picked.map(named)); return; }
    if (described) onChoose([refOf(described)]);
  };
  const footer = (
    <>
      <Button onClick={onClose}>Cancel</Button>
      <Button variant="primary" disabled={!multiple && !described} onClick={done}>{multiple ? "OK" : "Choose"}</Button>
    </>
  );
  const chosenLine = multiple ? picked.filter((p) => named(p).name) : [];

  const list = isSpellTarget(target) ? (
    <SpellPickerGrid rows={(data.data?.items ?? [])} total={data.data?.total ?? 0} text={filter.text} onText={(text) => setFilter({ ...filter, text })}
      picked={isPicked} highlighted={described?.id ?? null} onDescribe={setDescribed} onChoose={choose} multiple={multiple} />
  ) : (
    <FocusBar className="pick is-open">
      <Expander title={phone ? title : title.toUpperCase()} open onToggle={() => undefined}>
        <PickerFilterBar title={noun} filter={filter} onChange={setFilter} sources={sourcesOf(allRows)} editions={editions} shown={visible.length} total={data.data?.total ?? 0} />
        {target.types.length !== 1 && typeChoices.length > 1 && (
          <span className="hb-pick-type">
            <span className="pick-flabel">Type</span>
            <ComboBox aria-label="Element type" value={type} onChange={setType} options={[{ value: "", label: "Any type" }, ...typeChoices.map((t) => ({ value: t, label: t }))]} />
          </span>
        )}
        {data.error && <p className="pick-empty danger-text" role="alert">{data.error}</p>}
        {data.data && visible.length === 0 && <p className="pick-empty">Nothing matches.</p>}
        <ul className="pick-list with-desc hb-pick-list" aria-label={`${title} options`}>
          {visible.map((v) => (
            <PickerRow key={`${v.type}:${v.id}`} row={shownRow(v, isPicked(v.id))} listSlot={false} showDescription versionColumn={false}
              highlighted={described?.id === v.id} menuOpen={false} notesOpen={false} notesId={`hb-pick-notes-${v.id}`} onToggleNotes={() => undefined}
              onDescribe={() => setDescribed(v.row)} onChoose={() => choose(v.row)} onOpenMenu={() => undefined} onKeyNav={onKeyNav} />
          ))}
        </ul>
      </Expander>
      {(multiple ? chosenLine.length > 0 : chosen.length > 0) && (
        <div className="pick-footer">
          {(multiple ? chosenLine : picked.slice(0, 1)).map((p) => (
            <div key={p.id} className="pick-chosen">
              <Icon name="check-box" />
              <span className="pick-chosen-name">{named(p).name || "Chosen"}</span>
              {multiple && <button type="button" className="pick-x" aria-label={`Clear ${named(p).name}`} onClick={() => setPicked((cur) => cur.filter((x) => x.id !== p.id))}><Icon name="x" /></button>}
            </div>
          ))}
        </div>
      )}
    </FocusBar>
  );

  return (
    <Dialog title={multiple ? `Choose ${noun}` : `Choose: ${title}`} onClose={onClose} wide className="hb-picker" footer={footer}>
      <div className="hb-picker-body">
        <div className="hb-picker-list">{list}</div>
        <div className="hb-picker-desc">
          {described && !described.draft
            ? <DescriptionPane elementId={described.id} fallback={null} />
            : <aside className="desc-pane" aria-label="Description">{described && <p className="muted">{described.name}: {described.summary}</p>}</aside>}
          {phone && described && !multiple && <Button variant="primary" className="hb-picker-choose" onClick={done}>Choose {described.name}</Button>}
        </div>
      </div>
    </Dialog>
  );
}
