import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import type { HomebrewDraftView, HomebrewElementSummary, HomebrewForm, HomebrewKindDescriptor, HomebrewProblem } from "../../api/contract";
import { Button, Icon, useToast } from "../../components";
import { useNarrow } from "../../components/shell/useNarrow";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage, useAsync } from "../../state/useAsync";
import { ElementDetailView } from "../builder/DescriptionPane";
import { Editor, editionOf, editorTabs } from "./Editor";
import { Categories, ElementList, KindChips } from "./ElementList";
import { useCreatorKeys } from "./keymap";
import { ADD_BLOCK_EVENT } from "./blocks/BlockList";
import { type CopyChoice, NewElementFlyout, creatableKinds } from "./NewElementFlyout";
import { rememberPack } from "./packs";
import { countProblems, fieldDomId, fieldKeyOf, problemsLine, tabOf } from "./problems";
import { useDraft } from "./useDraft";
import { useMessageBox } from "./useMessageBox";
import "../builder/builder.css";
import "./homebrew.css";

const timeOf = (iso: string): string => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
};

/** Focuses a field's control (PROBLEMS rows, F2). */
function focusField(key: string, select = false): void {
  const box = document.getElementById(fieldDomId(key));
  const target = box?.querySelector<HTMLElement>("input, textarea, select, button, [tabindex]") ?? box;
  target?.focus();
  if (select && target instanceof HTMLInputElement) target.select();
}

/**
 * A homebrew pack's workspace (spec 13.2): CATEGORIES | element grid | editor | live description pane. Phones and narrow
 * windows (spec 16) show the list, then the editor with a back button; the kinds become chips and the description its own
 * PREVIEW sub-tab.
 */
export function PackWorkspace() {
  const packId = decodeURIComponent(useParams().packId ?? "");
  const api = useApi();
  const nav = useNavigate();
  const toast = useToast();
  const narrow = useNarrow();
  const [params, setParams] = useSearchParams();
  const kinds = useAsync(() => api.homebrewKinds(), [api]);
  const packs = useAsync(() => api.listPacks(), [api]);
  const elements = useAsync(() => api.homebrewListElements({ packId }), [api, packId]);
  const draft = useDraft(api);
  const { ask, box } = useMessageBox();
  const [kindPick, setKindPick] = useState<string | null>(null);
  const [tab, setTab] = useState("general");
  const [filter, setFilter] = useState("");
  const [newOpen, setNewOpen] = useState(false);
  const [tryItOpen, setTryItOpen] = useState(false);
  const [mobileView, setMobileView] = useState<"list" | "editor">("list");
  const filterRef = useRef<HTMLInputElement>(null);
  const pack = (packs.data ?? []).find((p) => p.id === packId) ?? null;

  useEffect(() => { rememberPack(packId); }, [packId]);
  // The problems line belongs to the open element: leaving the workspace clears it.
  useEffect(() => () => setStatus(""), []);
  // File > New homebrew element... lands here with ?new=1.
  useEffect(() => {
    if (params.get("new") !== "1") return;
    setNewOpen(true);
    setParams({}, { replace: true });
  }, [params, setParams]);

  const offered = useMemo(() => creatableKinds(kinds.data ?? []), [kinds.data]);
  const rows = useMemo(() => elements.data ?? [], [elements.data]);
  const counts = useMemo(() => {
    const m = new Map<string, number>();
    for (const r of rows) m.set(r.kind, (m.get(r.kind) ?? 0) + 1);
    return m;
  }, [rows]);
  const kindOf = useCallback((id: string | undefined): HomebrewKindDescriptor | null => (kinds.data ?? []).find((k) => k.id === id) ?? null, [kinds.data]);
  const openKind = kindOf(draft.form?.kind);
  const listKind = kindOf(kindPick ?? undefined) ?? openKind ?? offered.find((k) => counts.has(k.id)) ?? offered[0] ?? null;
  const listRows = rows.filter((r) => r.kind === listKind?.id);
  const openId = draft.view?.elementId ?? null;
  const unsaved = draft.form && !openId && draft.form.kind === listKind?.id
    ? { name: typeof draft.form.fields.name === "string" && draft.form.fields.name.trim() ? draft.form.fields.name.trim() : `New ${listKind.label.toLowerCase()}` }
    : null;

  // Status line (spec 8.3): "2 problems" while an element is open; "Checking..." while the engine checks run.
  const counted = countProblems(draft.problems);
  const hasForm = draft.form !== null;
  useEffect(() => {
    if (hasForm) setStatus(draft.checking ? "Checking..." : problemsLine(counted));
  }, [hasForm, draft.checking, counted.errors, counted.warnings]); // eslint-disable-line react-hooks/exhaustive-deps

  const show = async (view: HomebrewDraftView) => {
    setTryItOpen(false);
    await draft.open(view);
    setKindPick(view.form.kind);
    setTab(kindOf(view.form.kind)?.tabs[0]?.id ?? "general");
    setMobileView("editor");
  };

  /** Spec 10.1: a saved element that changed under the draft (another device's pack arrived) asks which one to keep. */
  const settleBase = async (view: HomebrewDraftView): Promise<HomebrewDraftView> => {
    if (!view.baseChanged || !view.elementId) return view;
    const r = await ask({
      title: "Element changed", cancelId: "keep",
      message: "This element changed since you started editing.",
      buttons: [{ id: "keep", label: "Keep my draft" }, { id: "saved", label: "Use the saved version" }],
    });
    if (r !== "saved") return view;
    await api.homebrewDiscardDraft({ draftId: view.draftId });
    return api.homebrewOpenDraft({ packId, elementId: view.elementId });
  };

  const openElement = async (row: HomebrewElementSummary) => {
    if (row.id === openId) { setMobileView("editor"); return; }
    try {
      if (row.hasDraft) {
        const saved = (await api.homebrewListDrafts()).find((d) => d.elementId === row.id);
        const r = await ask({
          title: "Unsaved changes", cancelId: "cancel",
          message: saved ? `Unsaved changes from ${timeOf(saved.updatedAt)}.` : "This element has unsaved changes.",
          buttons: [{ id: "keep", label: "Keep editing" }, { id: "discard", label: "Discard" }, { id: "cancel", label: "Cancel" }],
        });
        if (r === "cancel") return;
        if (r === "discard" && saved) { await api.homebrewDiscardDraft({ draftId: saved.draftId }); elements.reload(); }
      }
      await show(await settleBase(await api.homebrewOpenDraft({ packId, elementId: row.id })));
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };

  /** A new element (spec 10.1), or a copy (spec 11): a copy is named "<Name> (copy)", selected for renaming. */
  const createElement = async (kindId: string, name: string, copy?: CopyChoice) => {
    const fresh = await api.homebrewNewDraft({ packId, kind: kindId, ...(copy ? { copyFrom: copy.copyFrom, deepCopy: copy.deepCopy } : {}) });
    const view = name ? await api.homebrewUpdateDraft({ draftId: fresh.draftId, form: { ...fresh.form, fields: { ...fresh.form.fields, name } } }) : fresh;
    setNewOpen(false);
    await show(view);
    if (!name) setTimeout(() => focusField("name", !!copy), 0);
  };

  // "Copy to homebrew" (Compendium, builder) lands here with ?draft=<id>: the copy opens, its name selected for renaming.
  useEffect(() => {
    const draftId = params.get("draft");
    if (!draftId) return;
    setParams({}, { replace: true });
    void api.homebrewOpenDraft({ packId, draftId })
      .then(async (view) => { await show(view); setTimeout(() => focusField("name", true), 0); })
      .catch((e) => toast.push(errorMessage(e), "error"));
  }, [params]); // eslint-disable-line react-hooks/exhaustive-deps

  const newOfKind = () => { if (listKind) void createElement(listKind.id, "").catch((e) => toast.push(errorMessage(e), "error")); };

  const duplicate = async () => {
    if (!openId) return;
    try { await show(await api.homebrewDuplicateElement({ packId, elementId: openId })); } catch (e) { toast.push(errorMessage(e), "error"); }
  };

  const save = async () => {
    if (!draft.form) return;
    const name = typeof draft.form.fields.name === "string" ? draft.form.fields.name.trim() : "";
    try {
      const result = await draft.save();
      const id = result.elementIds[0];
      if (id) draft.rebase(await api.homebrewOpenDraft({ packId, elementId: id }));
      elements.reload();
      packs.reload();
      setStatus(`Saved '${name}' to ${result.pack.name}.`);
    } catch (e) {
      setStatus(errorMessage(e));
    }
  };

  const jump = (p: HomebrewProblem) => {
    const t = tabOf(p, openKind);
    if (t) setTab(t);
    const key = fieldKeyOf(p.path);
    if (key) setTimeout(() => focusField(key), 0);
    const block = p.path ? /^blocks\.(\d+)/.exec(p.path) : null;
    if (block) setTimeout(() => document.querySelector<HTMLElement>(`.hb-block-list [data-block="${block[1]}"]`)?.focus(), 0);
  };

  const tryIt = () => { if (draft.view) setTryItOpen(true); };

  /** A child of the open element (WB-16): what is pending is sent first, then the backend makes the child inside the
   * draft (`homebrew_new_draft {parentId}`), and its answer is one undoable edit (with `place`'s changes). */
  const newChild = async (kindId: string, place?: (form: HomebrewForm, key: string) => HomebrewForm): Promise<string | null> => {
    const draftId = draft.view?.draftId;
    if (!draftId) return null;
    try {
      if (!(await draft.flush())) return null;
      const view = await api.homebrewNewDraft({ packId, kind: kindId, parentId: draftId });
      const key = view.form.children[view.form.children.length - 1]?.key;
      if (!key) return null;
      draft.edit(place ? place(view.form, key) : view.form);
      return key;
    } catch (e) {
      toast.push(errorMessage(e), "error");
      return null;
    }
  };

  useCreatorKeys({
    save: () => void save(),
    undo: () => draft.undo(),
    redo: () => draft.redo(),
    newElement: () => newOfKind(),
    duplicate: () => void duplicate(),
    focusFilter: () => { setMobileView("list"); setTimeout(() => filterRef.current?.focus(), 0); },
    tryIt,
    tab: (a) => { const t = openKind ? editorTabs(openKind, narrow, editionOf(draft.form))[a.index ?? 0] : undefined; if (t) setTab(t.id); },
    rename: () => focusField("name", true),
    addBlock: () => {
      const rules = openKind?.fields.find((f) => f.control === "blocks");
      if (!rules || !draft.form) return;
      setTab(rules.tab);
      setTimeout(() => window.dispatchEvent(new Event(ADD_BLOCK_EVENT)), 0);
    },
  });

  const preview = draft.view?.preview
    ? <ElementDetailView d={draft.view.preview} />
    : <aside className="desc-pane" aria-label="Description" />;
  const editor = openKind && draft.form
    ? <Editor kind={openKind} kinds={kinds.data ?? []} draft={draft} tab={tab} onTab={setTab} narrow={narrow} preview={preview} tryItOpen={tryItOpen}
      actions={{ onSave: () => void save(), onTryIt: tryIt, onTryItClose: () => setTryItOpen(false), onDuplicate: () => void duplicate(), onJump: jump, onNewChild: newChild }} />
    : <div className="hb-empty"><p>Choose an element in the list, or make a new one.</p><Button onClick={() => setNewOpen(true)} disabled={offered.length === 0}>New element</Button></div>;
  const list = (
    <ElementList kind={listKind} rows={listRows} unsaved={unsaved} openId={openId} filter={filter} filterRef={filterRef} onFilter={setFilter}
      onOpen={(r) => void openElement(r)} onNew={() => setNewOpen(true)} onDuplicate={() => void duplicate()} />
  );
  const errors = [kinds.error, elements.error].filter(Boolean);
  return (
    <div className={`hb-ws ${narrow ? "narrow" : "wide"}`} data-view={narrow ? mobileView : undefined}>
      <h1 className="sr-only">{pack ? `Homebrew: ${pack.name}` : "Homebrew"}</h1>
      <div className="hb-ws-head">
        <button type="button" className="hb-back" onClick={() => (narrow && mobileView === "editor" ? setMobileView("list") : nav("/homebrew"))}>
          <Icon name="back" />{narrow && mobileView === "editor" ? `Back to ${listKind?.pluralLabel.toLowerCase() ?? "the list"}` : "All packs"}
        </button>
        <span className="hb-ws-pack">{pack?.name ?? ""}</span>
      </div>
      {errors.map((e) => <p key={e} className="hb-error-line" role="alert">{e}</p>)}
      {narrow ? (
        mobileView === "list"
          ? <div className="hb-ws-list"><KindChips kinds={offered} counts={counts} current={listKind?.id ?? null} onPick={setKindPick} />{list}</div>
          : <div className="hb-ws-editor">{editor}</div>
      ) : (
        <div className="hb-ws-grid">
          <div className="hb-ws-left">
            <Categories kinds={offered} counts={counts} current={listKind?.id ?? null} onPick={setKindPick} />
            {list}
            <div className="hb-ws-editor">{editor}</div>
          </div>
          <div className="hb-ws-desc">{preview}</div>
        </div>
      )}
      {newOpen && <NewElementFlyout kinds={kinds.data ?? []} initialKind={listKind?.id ?? null} onCreate={createElement} onClose={() => setNewOpen(false)} />}
      {box}
    </div>
  );
}
