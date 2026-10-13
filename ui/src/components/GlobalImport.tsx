import { useCallback, useEffect, useRef } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { classifyImport, RULES_IMPORT_ACCEPT, unsupportedImportMessage } from "../lib/importRouting";
import { base64ToFile, fileToBase64, filesToInputs } from "../lib/files";
import { ImportPreviewHost } from "../pages/content/ImportPreviewDialog";
import { startImportPreview } from "../state/importPreview";
import { useApi } from "../state/ApiContext";
import { stashImportFiles } from "../state/importInbox";
import { usePlatform } from "../state/platform";
import { errorMessage } from "../state/useAsync";
import { Icon } from "./Icon";
import { useImportNotice } from "./ImportNotice";
import { useToast } from "./Toast";

const hasFiles = (e: DragEvent) => Array.from(e.dataTransfer?.types ?? []).includes("Files");

/**
 * Floating "+" and a window-wide drop target (HTML drops in a browser, native drops via `files:dropped` in the app). Aurora XML, .index, .dnd5e and .json go straight to the rules-based importers
 * (no AI needed); other files (PDF, photo, text) go to the optional AI import when it is on, else get a neutral message.
 */
export function GlobalImport() {
  const api = useApi();
  const toast = useToast();
  const notice = useImportNotice();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const { aiEnabled, mobile } = usePlatform();
  const pickRef = useRef<HTMLInputElement>(null);
  const onImport = pathname === "/import";

  const route = useCallback(async (files: File[]) => {
    const others: File[] = [];
    const packs: File[] = [];
    let target: string | null = null;
    for (const f of files) {
      try {
        const kind = await classifyImport(f);
        if (kind === "pack") packs.push(f);
        else if (kind === "character") {
          const r = await api.importCharacter({ fileName: f.name, contentBase64: await fileToBase64(f) });
          toast.push(`Imported ${r.summary.name}.`);
          target ??= "/characters";
        } else others.push(f);
      } catch (e) { toast.push(errorMessage(e), "error"); }
    }
    if (packs.length && (await notice.ensure())) {
      // All content files from one pick or drop are converted together so references between them resolve.
      await startImportPreview(api, { files: await filesToInputs(packs) }, (m) => toast.push(m, "error"));
    }
    if (others.length && aiEnabled) { stashImportFiles(others); navigate("/import"); return; }
    if (others.length) toast.push(unsupportedImportMessage(others.map((f) => f.name), mobile), "error");
    if (target) navigate(target);
  }, [api, toast, navigate, aiEnabled, mobile, notice]);

  // Files the OS opened with Borealis ("Open with" / share): Tauri queues them (macOS, iOS); Android's activity pushes to window.__borealisOpened.
  useEffect(() => {
    const drain = async () => {
      const win = window as unknown as { __borealisOpened?: { fileName: string; contentBase64: string }[] };
      const queued = win.__borealisOpened ?? [];
      win.__borealisOpened = [];
      const opened = [...queued, ...(await api.takeOpenedFiles().catch(() => []))];
      if (opened.length) void route(opened.map((o) => base64ToFile(o.fileName, o.contentBase64)));
    };
    void drain();
    const off = api.subscribe("content:open-file", () => void drain());
    const onOpen = () => void drain();
    window.addEventListener("borealis-open-file", onOpen);
    return () => { off(); window.removeEventListener("borealis-open-file", onOpen); };
  }, [api, route]);

  // Files dropped from the OS on the desktop app: the backend handles the native drop and sends tokens and names only.
  useEffect(() => api.subscribe("files:dropped", (drop) => {
    void (async () => {
      if (drop.rejected.length) toast.push(unsupportedImportMessage(drop.rejected, mobile), "error");
      if (!drop.files.length) return;
      try {
        const opened = await api.takePickedFiles({ purpose: "dropped-file", tokens: drop.files.map((f) => f.token) });
        await route(opened.map((o) => base64ToFile(o.fileName, o.contentBase64)));
      } catch (e) { toast.push(errorMessage(e), "error"); }
    })();
  }), [api, route, toast, mobile]);

  useEffect(() => {
    const over = (e: DragEvent) => { if (hasFiles(e)) e.preventDefault(); };
    const drop = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      if (onImport) return;
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length) void route(files);
    };
    window.addEventListener("dragover", over);
    window.addEventListener("drop", drop);
    return () => { window.removeEventListener("dragover", over); window.removeEventListener("drop", drop); };
  }, [route, onImport]);

  if (onImport) return <ImportPreviewHost />;
  if (aiEnabled) return <><ImportPreviewHost /><Link to="/import" className="fab-import" aria-label="Import content" title="Import content"><Icon name="plus" /></Link></>;
  return (
    <>
      <ImportPreviewHost />
      <input ref={pickRef} type="file" multiple accept={RULES_IMPORT_ACCEPT} hidden aria-label="Choose files to import"
        onChange={(e) => { const files = Array.from(e.target.files ?? []); e.target.value = ""; if (files.length) void route(files); }} />
      <button type="button" className="fab-import" aria-label="Import content" title="Import content" onClick={() => pickRef.current?.click()}><Icon name="plus" /></button>
    </>
  );
}
