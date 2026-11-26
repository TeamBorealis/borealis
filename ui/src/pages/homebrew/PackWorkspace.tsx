import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import type { HomebrewDraftSummary, HomebrewDraftView, HomebrewElementSummary, HomebrewForm, HomebrewKindDescriptor, HomebrewProblem } from "../../api/contract";
import { Button, Icon, useToast } from "../../components";
import { useNarrow } from "../../components/shell/useNarrow";
import { useApi } from "../../state/ApiContext";
import { haptic } from "../../lib/haptics";
import { setStatus } from "../../state/status";
import { errorMessage, useAsync } from "../../state/useAsync";
import { DescriptionPane, ElementDetailView, EmptyPane } from "../builder/DescriptionPane";
import { Editor, editionOf, editorTabs } from "./Editor";
import { BulkActions, type BulkDone } from "./BulkActions";
import { Categories, ElementList, type ExtraCategory, KindChips } from "./ElementList";
import { KeysDialog } from "./KeysDialog";
import { EMPTY_FILTER, type ListFilter } from "./search";
import { keptChecks, keptKnown, loadKeptChecks } from "./keptLists";
import { SpellLists, type SpellListsHandle } from "./SpellLists";
import { WhereUsed } from "./WhereUsed";
import { useCreatorKeys } from "./keymap";
import { ADD_BLOCK_EVENT } from "./blocks/BlockList";
import { type CopyChoice, NewElementFlyout, creatableKinds } from "./NewElementFlyout";
import { rememberPack } from "./packs";
import { countProblems, creatorStatus, fieldDomId, fieldKeyOf, tabOf } from "./problems";
import { useDraft } from "./useDraft";
import { findForm } from "./class/children";
import { useMessageBox } from "./useMessageBox";
import { noteStatus, onStatusNote, settleNote, type ShownNote } from "./statusNote";
import "../builder/builder.css";
import "./homebrew.css";
import "./expert.css";

/** `content-reloaded` reasons of a pack changed from outside this page: a device receive, an imported pack file. */
const PACK_CHANGED = new Set(["device-sync", "homebrew-pack-imported"]);


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
  /** The device's drafts: a new element never saved is listed from its draft (spec 10.1, item IDpe8). */
  const drafts = useAsync(() => api.homebrewListDrafts(), [api, packId]);
  /** Drafts this page saved or discarded: gone at once, while the list of drafts is still being read anew. */
  const [closed, setClosed] = useState<ReadonlySet<string>>(new Set());
  const closeDraft = (draftId: string) => setClosed((prev) => new Set([...prev, draftId]));
  const { ask, box } = useMessageBox();
  const [kindPick, setKindPick] = useState<string | null>(null);
  const [tab, setTab] = useState("general");
  const [filter, setFilter] = useState<ListFilter>(EMPTY_FILTER);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [selectMode, setSelectMode] = useState(false);
  /** The workspace shows a kind's elements, or the pack's SPELL LISTS (spec 12.4). */
  // unsaved SPELL LISTS checks kept from the last visit open SPELL LISTS again (P3)
  const [pane, setPane] = useState<"elements" | "lists">(() => (keptChecks(api, packId) ? "lists" : "elements"));
  // ...and so do those kept on the device by an earlier run of the app (INW5c), unless an element was opened meanwhile
  const openedSince = useRef(false);
  useEffect(() => { if (draft.view) openedSince.current = true; }, [draft.view]);
  useEffect(() => {
    if (keptKnown(api, packId)) return undefined;
    let live = true;
    loadKeptChecks(api, packId)
      .then((k) => { if (live && k && !openedSince.current) setPane("lists"); })
      .catch((e) => { if (live) toast.push(errorMessage(e), "error"); });
    return () => { live = false; };
  }, [api, packId]); // eslint-disable-line react-hooks/exhaustive-deps
  const [described, setDescribed] = useState<string | null>(null);
  const [keysOpen, setKeysOpen] = useState(false);
  /** Bumped after every write: WHERE USED reads again. */
  const [written, setWritten] = useState(0);
  const lists = useRef<SpellListsHandle | null>(null);
  const [newOpen, setNewOpen] = useState(false);
  const [tryItOpen, setTryItOpen] = useState(false);
  const [mobileView, setMobileView] = useState<"list" | "editor">("list");
  const filterRef = useRef<HTMLInputElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const pack = (packs.data ?? []).find((p) => p.id === packId) ?? null;
  // A pack opened from the list (or just made in New pack) leaves focus nowhere: the opener is gone. Keyboard and screen
  // reader users start at the pack's heading, "Homebrew: <name>" (WB-26).
  useEffect(() => {
    if (!pack) return;
    const active = document.activeElement;
    if (!active || active === document.body) headingRef.current?.focus();
  }, [packId, pack !== null]); // eslint-disable-line react-hooks/exhaustive-deps

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
  const openId = draft.view?.elementId ?? null;
  const openDraftId = draft.view?.draftId ?? null;
  const newDrafts = useMemo(
    () => (drafts.data ?? []).filter((d) => d.packId === packId && d.elementId === null && d.draftId !== openDraftId && !closed.has(d.draftId)),
    [drafts.data, packId, openDraftId, closed],
  );
  // The never-saved element on screen, if any: listed under its kind like the new drafts left in the list.
  const openNewKind = draft.form && !openId ? draft.form.kind : null;
  const counts = useMemo(() => {
    const m = new Map<string, number>();
    const add = (kind: string) => m.set(kind, (m.get(kind) ?? 0) + 1);
    rows.forEach((r) => add(r.kind));
    // H10MQ: a new element counts in its kind from the start ("Weapons (1)"), as the list shows it, not after its first save
    newDrafts.forEach((d) => add(d.kind));
    if (openNewKind) add(openNewKind);
    return m;
  }, [rows, newDrafts, openNewKind]);
  const kindOf = useCallback((id: string | undefined): HomebrewKindDescriptor | null => (kinds.data ?? []).find((k) => k.id === id) ?? null, [kinds.data]);
  const openKind = kindOf(draft.form?.kind);
  const listKind = kindOf(kindPick ?? undefined) ?? openKind ?? offered.find((k) => counts.has(k.id)) ?? offered[0] ?? null;
  // The editor moved (another element, a save, a new element): the draft it left was sent first, so read the list again.
  useEffect(() => { drafts.reload(); }, [openDraftId]); // eslint-disable-line react-hooks/exhaustive-deps
  const unsaved = draft.form && !openId && draft.form.kind === listKind?.id
    ? { name: typeof draft.form.fields.name === "string" && draft.form.fields.name.trim() ? draft.form.fields.name.trim() : `New ${listKind.label.toLowerCase()}`,
      edition: editionOf(draft.form) }
    : null;

  // Status line (spec 8.3): "2 problems" while an element is open; "Checking..." while the engine checks run. After a save
  // (HC18) it keeps "Saved 'X' to Pack" and adds the count, until the form on screen changes (an edit, another element).
  // Every creator message is kept the same way ("Made a copy...", "Rule added.": `noteStatus`).
  const [note, setNote] = useState<ShownNote | null>(null);
  useEffect(() => onStatusNote(setNote), []);
  useEffect(() => {
    if (!note) return;
    // The note belongs to the form on screen when it first shows (the element reopened after a save, the rule just added);
    // with no element open the message is the whole line. The change applies only to the note this effect saw: a newer
    // message written meanwhile is kept (settleNote).
    if (note.form === undefined && !draft.form) setStatus(note.text);
    const form = draft.form;
    setNote((current) => settleNote(current, note, form));
  }, [note, draft.form]);
  const counted = countProblems(draft.problems);
  const hasForm = draft.form !== null;
  useEffect(() => {
    if (!hasForm) return;
    const line = creatorStatus(note?.text ?? null, draft.checking, counted, note?.count ?? true);
    setStatus(line.message, line.detail);
  }, [hasForm, note?.text, note?.count, draft.checking, counted.errors, counted.warnings]); // eslint-disable-line react-hooks/exhaustive-deps

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

  // Spec 14.4: a pack received from another device, or replaced by an imported pack file, while an element is open: the
  // element opens again from the pack (the list and pack too); a draft of it asks which version to keep (10.1).
  const onPackChanged = useRef<() => void>(() => undefined);
  onPackChanged.current = () => {
    elements.reload();
    packs.reload();
    const id = openId;
    if (!id) return;
    void draft.flush()
      .then(() => api.homebrewOpenDraft({ packId, elementId: id }))
      .then(settleBase).then(show)
      .catch(() => draft.open(null));
  };
  useEffect(() => api.subscribe("content:reloaded", ({ reason }) => {
    if (PACK_CHANGED.has(reason)) onPackChanged.current();
  }), [api]);

  /** Spec 10.1: reopening an element with a draft asks "Unsaved changes from <time>: Keep editing / Discard". */
  const askUnsaved = (saved: HomebrewDraftSummary | undefined) => ask({
    title: "Unsaved changes", cancelId: "cancel",
    message: saved ? `Unsaved changes from ${timeOf(saved.updatedAt)}.` : "This element has unsaved changes.",
    buttons: [{ id: "keep", label: "Keep editing" }, { id: "discard", label: "Discard" }, { id: "cancel", label: "Cancel" }],
  });

  const openElement = async (row: HomebrewElementSummary) => {
    if (row.id === openId) { setMobileView("editor"); return; }
    try {
      if (row.hasDraft) {
        const saved = (await api.homebrewListDrafts()).find((d) => d.elementId === row.id);
        const r = await askUnsaved(saved);
        if (r === "cancel") return;
        if (r === "discard" && saved) { await api.homebrewDiscardDraft({ draftId: saved.draftId }); elements.reload(); }
      }
      await show(await settleBase(await api.homebrewOpenDraft({ packId, elementId: row.id })));
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };

  /** A new element that was never saved: Keep editing resumes its draft by id; Discard drops it (item IDpe8). */
  const openNewDraft = async (saved: HomebrewDraftSummary) => {
    try {
      const r = await askUnsaved(saved);
      if (r === "cancel") return;
      if (r === "discard") {
        await api.homebrewDiscardDraft({ draftId: saved.draftId });
        closeDraft(saved.draftId);
        drafts.reload();
        packs.reload();
        return;
      }
      await show(await api.homebrewOpenDraft({ packId, draftId: saved.draftId }));
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };

  /** Open a saved element of the pack by id (WHERE USED, `?element=`, after a multi-select edit). */
  const openById = async (id: string) => {
    const row = rows.find((r) => r.id === id);
    if (row) { setPane("elements"); await openElement(row); return; }
    try { setPane("elements"); await show(await settleBase(await api.homebrewOpenDraft({ packId, elementId: id }))); } catch (e) { toast.push(errorMessage(e), "error"); }
  };

  /** Leave SPELL LISTS: unsaved checks ask first. False when the user stays. */
  const leaveLists = async (): Promise<boolean> => {
    if (pane === "lists" && (lists.current?.pending() ?? 0) > 0) {
      const r = await ask({
        title: "Unsaved changes", cancelId: "cancel", message: "Changes to the spell list are not saved.",
        buttons: [{ id: "save", label: "Save" }, { id: "discard", label: "Discard" }, { id: "cancel", label: "Cancel" }],
      });
      if (r === "cancel") return false;
      if (r === "save") await lists.current?.save();
      else lists.current?.discard();
    }
    setPane("elements");
    return true;
  };

  /** Leave SPELL LISTS for a kind. */
  const pickKind = async (kindId: string) => {
    if (!(await leaveLists())) return;
    setKindPick(kindId);
    setSelected(new Set());
  };
  const extra: ExtraCategory[] = [{ label: "Spell Lists", current: pane === "lists", onPick: () => { setPane("lists"); setMobileView("list"); } }];

  /** After a multi-select edit or its undo: read the pack again; the open element closes when it left the pack, else it is
   * opened again (a draft of it asks which version to keep, spec 10.1). The message shows once the element on screen is
   * settled, so it stays with it (HC18); it is about the pack, so it takes no problems count. */
  const bulkDone = (done: BulkDone) => {
    elements.reload();
    packs.reload();
    setWritten((n) => n + 1);
    setSelected(new Set());
    const tell = () => noteStatus(done.note, { count: false });
    if (!openId || !done.ids.includes(openId)) { tell(); return; }
    if (done.action === "delete" || done.action === "move") { void draft.open(null).then(tell); return; }
    // the open element changed under its form: open it again from the pack
    const id = openId;
    void api.homebrewOpenDraft({ packId, elementId: id }).then(settleBase).then(show).catch(() => draft.open(null)).then(tell);
  };

  /** A new element (spec 10.1), or a copy (spec 11): a copy is named "<Name> (copy)", selected for renaming. */
  const createElement = async (kindId: string, name: string, copy?: CopyChoice) => {
    if (copy?.override) {
      // HC20 (spec 22): an override keeps the original's name and id
      const view = await api.homebrewNewOverride({ packId, elementId: copy.copyFrom });
      setNewOpen(false);
      await show(view);
      setTimeout(() => focusField("name", false), 0);
      return;
    }
    const fresh = await api.homebrewNewDraft({ packId, kind: kindId, ...(copy ? { copyFrom: copy.copyFrom, deepCopy: copy.deepCopy } : {}) });
    const view = name ? await api.homebrewUpdateDraft({ draftId: fresh.draftId, form: { ...fresh.form, fields: { ...fresh.form.fields, name } } }) : fresh;
    setNewOpen(false);
    await show(view);
    // keyboard users land in the new element's form, not back on New (WB-26); a copy's name is selected for renaming
    setTimeout(() => focusField("name", !!copy), 0);
  };

  // WHERE USED in another pack lands here with ?element=<id>.
  useEffect(() => {
    const elementId = params.get("element");
    if (!elementId || !elements.data) return;
    setParams({}, { replace: true });
    void openById(elementId);
  }, [params, elements.data]); // eslint-disable-line react-hooks/exhaustive-deps

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
    const savedDraft = draft.view?.draftId;
    try {
      const result = await draft.save();
      if (savedDraft) closeDraft(savedDraft);
      const id = result.elementIds[0];
      if (id) draft.rebase(await api.homebrewOpenDraft({ packId, elementId: id }));
      elements.reload();
      packs.reload();
      setWritten((n) => n + 1);
      haptic("save");
      noteStatus(`Saved '${name}' to ${result.pack.name}`);
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
  const newChild = async (kindId: string, place?: (form: HomebrewForm, key: string) => HomebrewForm, parentKey?: string): Promise<string | null> => {
    const draftId = draft.view?.draftId;
    if (!draftId) return null;
    try {
      if (!(await draft.flush())) return null;
      // `parentKey`: inside that child of the draft (a variant's own feature, a trait's lineage option)
      const view = await api.homebrewNewDraft({ packId, kind: kindId, parentId: draftId, ...(parentKey ? { parentKey } : {}) });
      const holder = parentKey ? findForm(view.form, parentKey) : view.form;
      const key = holder?.children[holder.children.length - 1]?.key;
      if (!key) return null;
      draft.edit(place ? place(view.form, key) : view.form);
      return key;
    } catch (e) {
      toast.push(errorMessage(e), "error");
      return null;
    }
  };

  /** Before a saved child leaves its parent (it is deleted on save): the local characters that use it, and the question
   * element delete asks (spec 10.3). */
  const confirmRemove = async (childKey: string, name: string): Promise<boolean> => {
    const draftId = draft.view?.draftId;
    if (!draftId) return true;
    try {
      if (!(await draft.flush())) return false;
      const used = await api.homebrewChildUse({ draftId, childKey });
      if (used.length === 0) return true;
      const who = `${used.length} character${used.length === 1 ? "" : "s"}`;
      const r = await ask({
        title: "Remove", cancelId: "cancel",
        message: `${who} use ${name}: ${used.map((c) => c.name).join(", ")}. They will lose it when you save. Remove anyway?`,
        buttons: [{ id: "remove", label: "Remove", variant: "danger" }, { id: "cancel", label: "Cancel" }],
      });
      return r === "remove";
    } catch (e) {
      toast.push(errorMessage(e), "error");
      return false;
    }
  };

  useCreatorKeys({
    save: () => void (pane === "lists" ? lists.current?.save() : save()),
    undo: () => draft.undo(),
    redo: () => draft.redo(),
    newElement: () => newOfKind(),
    duplicate: () => void duplicate(),
    focusFilter: () => { setPane("elements"); setMobileView("list"); setTimeout(() => filterRef.current?.focus(), 0); },
    tryIt,
    tab: (a) => { const t = openKind ? editorTabs(openKind, narrow, editionOf(draft.form), draft.form)[a.index ?? 0] : undefined; if (t) setTab(t.id); },
    rename: () => focusField("name", true),
    addBlock: () => {
      const rules = openKind?.fields.find((f) => f.control === "blocks");
      if (!rules || !draft.form) return;
      setTab(rules.tab);
      setTimeout(() => window.dispatchEvent(new Event(ADD_BLOCK_EVENT)), 0);
    },
  });

  const homebrewIds = new Set((packs.data ?? []).filter((p) => p.kind === "homebrew").map((p) => p.id));
  const openRef = (r: { id: string; packId: string | null; draftId?: string }) => {
    const { draftId } = r;
    if (draftId && r.packId === packId) {
      void leaveLists().then(async (left) => { if (left) await show(await api.homebrewOpenDraft({ packId, draftId })); })
        .catch((e) => toast.push(errorMessage(e), "error"));
    } else if (draftId) nav(`/homebrew/${encodeURIComponent(r.packId ?? "")}?draft=${encodeURIComponent(draftId)}`);
    else if (r.packId === packId) void openById(r.id);
    else nav(`/homebrew/${encodeURIComponent(r.packId ?? "")}?element=${encodeURIComponent(r.id)}`);
  };
  // HC5: Edit on a loaded element makes a deep copy in this pack and opens it, its name selected, as Copy to homebrew does.
  // The message follows the copy on screen, so it stays with it (HC18).
  const editCopy = async (r: { id: string; name: string }) => {
    try {
      if (draft.form && !(await draft.flush())) return;
      if (!(await leaveLists())) return;
      await show(await api.homebrewNewDraft({ packId, kind: "other", copyFrom: r.id, deepCopy: true }));
      setTimeout(() => focusField("name", true), 0);
      noteStatus(`Made a copy of '${r.name}' to edit. The original stays as it is.`);
    } catch (e) {
      toast.push(errorMessage(e), "error");
    }
  };
  const whereUsed = (id: string | null) => (
    <WhereUsed elementId={id} version={written} homebrewPackIds={homebrewIds} onOpen={openRef} onEdit={(r) => void editCopy(r)} />
  );
  const preview = (
    <>
      {draft.view?.preview ? <ElementDetailView d={draft.view.preview} /> : <EmptyPane />}
      {whereUsed(openId)}
    </>
  );
  const spellPane = (
    <>
      <DescriptionPane elementId={described} fallback={<aside className="desc-pane" aria-label="Description" />} />
      {whereUsed(described)}
    </>
  );
  const spellLists = <SpellLists key={packId} packId={packId} narrow={narrow} described={described} onDescribe={setDescribed} handle={lists} onSaved={() => setWritten((n) => n + 1)} />;
  const editor = openKind && draft.form
    ? <Editor kind={openKind} kinds={kinds.data ?? []} draft={draft} tab={tab} onTab={setTab} narrow={narrow} preview={preview} tryItOpen={tryItOpen}
      actions={{ onSave: () => void save(), onTryIt: tryIt, onTryItClose: () => setTryItOpen(false), onDuplicate: () => void duplicate(), onJump: jump, onNewChild: newChild, onConfirmRemove: confirmRemove,
        onPackChanged: () => { elements.reload(); packs.reload(); setWritten((n) => n + 1); },
        overrideSaved: rows.some((r) => r.id === openId && r.override),
        onReverted: () => { void draft.open(null); elements.reload(); packs.reload(); setWritten((n) => n + 1); } }} />
    : <div className="hb-empty"><p>Choose an element in the list, or make a new one.</p><Button onClick={() => setNewOpen(true)} disabled={offered.length === 0}>New element</Button></div>;
  const chosen = rows.filter((r) => selected.has(r.id));
  const list = (
    <div className="hb-list-col">
      <ElementList kind={listKind} kinds={offered} rows={rows} unsaved={unsaved} drafts={newDrafts} onOpenDraft={(d) => void openNewDraft(d)} openId={openId} filter={filter} filterRef={filterRef} onFilter={setFilter}
        onOpen={(r) => void openElement(r)} onNew={() => setNewOpen(true)} onDuplicate={() => void duplicate()}
        selected={selected} onSelect={setSelected} selectMode={selectMode} onSelectMode={(on) => { setSelectMode(on); if (!on) setSelected(new Set()); }} />
      {(chosen.length > 1 || (selectMode && chosen.length > 0)) && (
        <BulkActions packId={packId} chosen={chosen} packs={packs.data ?? []} onDone={bulkDone} onClear={() => setSelected(new Set())}
          beforeChange={async () => { if (draft.form) await draft.flush(); }} />
      )}
    </div>
  );
  const errors = [kinds.error, elements.error].filter(Boolean);
  return (
    <div className={`hb-ws ${narrow ? "narrow" : "wide"}`} data-view={narrow ? mobileView : undefined}>
      <h1 className="sr-only" tabIndex={-1} ref={headingRef}>{pack ? `Homebrew: ${pack.name}` : "Homebrew"}</h1>
      <div className="hb-ws-head">
        <button type="button" className="hb-back" onClick={() => (narrow && mobileView === "editor" ? setMobileView("list") : nav("/homebrew"))}>
          <Icon name="back" />{narrow && mobileView === "editor" ? `Back to ${listKind?.pluralLabel.toLowerCase() ?? "the list"}` : "All packs"}
        </button>
        <span className="hb-ws-pack">{pack?.name ?? ""}</span>
        {!narrow && <button type="button" className="hb-tool hb-keys-btn" title="Keyboard shortcuts" onClick={() => setKeysOpen(true)}>Keys</button>}
      </div>
      {errors.map((e) => <p key={e} className="hb-error-line" role="alert">{e}</p>)}
      {narrow ? (
        mobileView === "list"
          ? (
            <div className="hb-ws-list">
              <KindChips kinds={offered} counts={counts} current={listKind?.id ?? null} onPick={(k) => void pickKind(k)} extra={extra} />
              {pane === "lists" ? <>{spellLists}<div className="hb-ws-desc">{spellPane}</div></> : list}
            </div>
          )
          : <div className="hb-ws-editor">{editor}</div>
      ) : (
        <div className="hb-ws-grid">
          <div className={`hb-ws-left ${pane === "lists" ? "hb-lists-view" : ""}`}>
            <Categories kinds={offered} counts={counts} current={listKind?.id ?? null} onPick={(k) => void pickKind(k)} extra={extra} />
            {pane === "lists" ? spellLists : <>{list}<div className="hb-ws-editor">{editor}</div></>}
          </div>
          <div className="hb-ws-desc">{pane === "lists" ? spellPane : preview}</div>
        </div>
      )}
      {keysOpen && <KeysDialog onClose={() => setKeysOpen(false)} />}
      {newOpen && <NewElementFlyout kinds={kinds.data ?? []} initialKind={listKind?.id ?? null} onCreate={createElement} onClose={() => setNewOpen(false)} />}
      {box}
    </div>
  );
}
