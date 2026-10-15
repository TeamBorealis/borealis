import { useCallback, useEffect, useRef, useState } from "react";
import type { ContentPack, ImportDraft, ImportElement, ImportEvent, ImportFinalizeRequest, ImportStartRequest } from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { useWithCodexConsent } from "../../state/codexConsent";
import { errorMessage } from "../../state/useAsync";

export type ImportPhase = "source" | "progress" | "review" | "done";

export interface ImportRun {
  phase: ImportPhase;
  events: ImportEvent[];
  /** Elements as they stream in during generation. */
  liveElements: ImportElement[];
  draft: ImportDraft | null;
  error: string | null;
  /** Element ids (or "*") being revised right now. */
  revising: ReadonlySet<string>;
  pack: ContentPack | null;
  busy: boolean;
  start: (req: ImportStartRequest) => Promise<void>;
  choosePar: (classId: string) => Promise<void>;
  revise: (instruction: string, elementId?: string) => Promise<void>;
  updateElement: (elementId: string, json: Record<string, unknown>) => Promise<boolean>;
  /** Mark a flagged element as fine (or undo it). The draft is kept on failure. */
  accept: (elementId: string, accepted: boolean) => Promise<boolean>;
  /** Manual text-only override: true/false, or null to go back to automatic. */
  setTextOnly: (elementId: string, textOnly: boolean | null) => Promise<boolean>;
  /** Rebuild one element from the given 1-based inclusive source lines (streams like revise). */
  relocate: (elementId: string, startLine: number, endLine: number) => Promise<void>;
  /** Generate one element again from its current passage (streams like revise). */
  regenerate: (elementId: string) => Promise<void>;
  /** The last failed per-element action, shown next to that element. */
  elementError: { elementId: string; message: string } | null;
  finalize: (req: ImportFinalizeRequest) => Promise<ContentPack | null>;
  discard: () => Promise<void>;
}

/** Drives the whole import wizard: start, stream, review, revise, finalize. */
export function useImport(): ImportRun {
  const api = useApi();
  const withConsent = useWithCodexConsent();
  const [phase, setPhase] = useState<ImportPhase>("source");
  const [events, setEvents] = useState<ImportEvent[]>([]);
  const [draft, setDraft] = useState<ImportDraft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revising, setRevising] = useState<ReadonlySet<string>>(new Set());
  const [elementError, setElementError] = useState<ImportRun["elementError"]>(null);
  const [pack, setPack] = useState<ContentPack | null>(null);
  const [busy, setBusy] = useState(false);
  const idRef = useRef<string | null>(null);
  const phaseRef = useRef<ImportPhase>("source");
  const early = useRef<ImportEvent[]>([]);
  phaseRef.current = phase;

  const refetch = useCallback(async () => {
    if (!idRef.current) return;
    try { setDraft(await api.importGetDraft({ draftId: idRef.current })); } catch (e) { setError(errorMessage(e)); }
  }, [api]);

  const apply = useCallback((e: ImportEvent) => {
    setEvents((cur) => [...cur, e]);
    if (e.stage === "error") { setError(e.message); setRevising(new Set()); setPhase((p) => (p === "progress" ? "source" : p)); return; }
    if (e.stage === "ready") {
      setRevising(new Set());
      void refetch().then(() => setPhase((p) => (p === "progress" ? "review" : p)));
    }
  }, [refetch]);

  useEffect(() => api.subscribe("import:event", (e) => {
    if (!idRef.current) { early.current.push(e); return; }
    if (e.draftId === idRef.current) apply(e);
  }), [api, apply]);

  const start = useCallback(async (req: ImportStartRequest) => {
    setError(null); setBusy(true); setEvents([]); setDraft(null);
    early.current = [];
    idRef.current = null;
    try {
      const { draftId } = await withConsent((allowUntrustedWithCodex) => api.importStart(allowUntrustedWithCodex ? { ...req, allowUntrustedWithCodex } : req));
      idRef.current = draftId;
      setPhase("progress");
      const buffered = early.current.filter((e) => e.draftId === draftId);
      early.current = [];
      buffered.forEach(apply);
    } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  }, [api, apply, withConsent]);

  const choosePar = useCallback(async (classId: string) => {
    if (!idRef.current) return;
    const draftId = idRef.current;
    try { setDraft(await withConsent((allow) => api.importChooseParent({ draftId, classId, ...(allow ? { allowUntrustedWithCodex: true } : {}) }))); } catch (e) { setError(errorMessage(e)); }
  }, [api, withConsent]);

  const revise = useCallback(async (instruction: string, elementId?: string) => {
    if (!idRef.current) return;
    setError(null);
    setRevising((cur) => new Set(cur).add(elementId ?? "*"));
    const draftId = idRef.current;
    try { await withConsent((allow) => api.importRevise({ draftId, instruction, ...(elementId ? { elementId } : {}), ...(allow ? { allowUntrustedWithCodex: true } : {}) })); }
    catch (e) { setError(errorMessage(e)); setRevising(new Set()); }
  }, [api, withConsent]);

  const updateElement = useCallback(async (elementId: string, json: Record<string, unknown>) => {
    if (!idRef.current) return false;
    try { setDraft(await api.importUpdateElement({ draftId: idRef.current, elementId, json })); return true; }
    catch (e) { setError(errorMessage(e)); return false; }
  }, [api]);

  const failElement = useCallback((elementId: string, what: string, e: unknown) => {
    setElementError({ elementId, message: `Could not ${what}. Your draft is unchanged. ${errorMessage(e)}` });
  }, []);

  const accept = useCallback(async (elementId: string, accepted: boolean) => {
    if (!idRef.current) return false;
    setElementError(null);
    try { setDraft(await api.importAcceptElement({ draftId: idRef.current, elementId, accepted })); return true; }
    catch (e) { failElement(elementId, accepted ? "mark this as fine" : "undo that", e); return false; }
  }, [api, failElement]);

  const setTextOnly = useCallback(async (elementId: string, textOnly: boolean | null) => {
    if (!idRef.current) return false;
    setElementError(null);
    try { setDraft(await api.importSetTextOnly({ draftId: idRef.current, elementId, textOnly })); return true; }
    catch (e) { failElement(elementId, "change the text-only setting", e); return false; }
  }, [api, failElement]);

  const relocate = useCallback(async (elementId: string, startLine: number, endLine: number) => {
    if (!idRef.current) return;
    setElementError(null);
    setRevising((cur) => new Set(cur).add(elementId));
    const draftId = idRef.current;
    try { await withConsent((allow) => api.importRelocateElement({ draftId, elementId, startLine, endLine, ...(allow ? { allowUntrustedWithCodex: true } : {}) })); }
    catch (e) { failElement(elementId, "rebuild this element from those lines", e); setRevising(new Set()); }
  }, [api, withConsent, failElement]);

  const regenerate = useCallback(async (elementId: string) => {
    if (!idRef.current) return;
    setElementError(null);
    setRevising((cur) => new Set(cur).add(elementId));
    const draftId = idRef.current;
    try { await withConsent((allow) => api.importRegenerateElement({ draftId, elementId, ...(allow ? { allowUntrustedWithCodex: true } : {}) })); }
    catch (e) { failElement(elementId, "regenerate this element", e); setRevising(new Set()); }
  }, [api, withConsent, failElement]);

  const finalize = useCallback(async (req: ImportFinalizeRequest) => {
    if (!idRef.current) return null;
    setBusy(true); setError(null);
    try {
      const p = await api.importFinalize({ draftId: idRef.current, ...req });
      setPack(p); setPhase("done");
      return p;
    } catch (e) { setError(errorMessage(e)); return null; } finally { setBusy(false); }
  }, [api]);

  const discard = useCallback(async () => {
    const id = idRef.current;
    idRef.current = null;
    setPhase("source"); setDraft(null); setEvents([]); setError(null); setElementError(null); setPack(null); setRevising(new Set());
    if (id) await api.importDiscard({ draftId: id }).catch(() => undefined);
  }, [api]);

  const liveElements = events.flatMap((e) => (e.element ? [e.element] : []));
  return { phase, events, liveElements, draft, error, revising, pack, busy, start, choosePar, revise, updateElement, accept, setTextOnly, relocate, regenerate, elementError, finalize, discard };
}
