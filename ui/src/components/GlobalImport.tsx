import { useCallback, useEffect, useLayoutEffect, useRef } from "react";
import { Link, useLocation, useNavigate } from "react-router";
import { classifyImport, RULES_IMPORT_ACCEPT, unsupportedImportMessage } from "../lib/importRouting";
import { base64ToFile, fileToBase64, filesToInputs } from "../lib/files";
import { ImportPreviewHost } from "../pages/content/ImportPreviewDialog";
import { openBundleFile, openOsBundles } from "../pages/transfer/openBundle";
import { startImportPreview } from "../state/importPreview";
import { useApi } from "../state/ApiContext";
import { stashImportFiles } from "../state/importInbox";
import { routedMessage, sendToFiveTools } from "../state/fiveToolsInbox";
import { usePlatform } from "../state/platform";
import { errorMessage } from "../state/useAsync";
import { Icon } from "./Icon";
import { useImportNotice } from "./ImportNotice";
import { useToast } from "./Toast";

const hasFiles = (e: DragEvent) => Array.from(e.dataTransfer?.types ?? []).includes("Files");

/**
 * Floating "+" and a window-wide drop target (HTML drops in a browser, native drops via `files:dropped` in the app). Aurora XML, .index, .dnd5e and .json go straight to the rules-based importers
 * (a 5etools-format homebrew .json to the 5etools-format import)
 * (no AI needed); other files (PDF, photo, text) go to the optional AI import when it is on, else get a neutral message.
 */
export function GlobalImport() {
  const api = useApi();
  const toast = useToast();
  const notice = useImportNotice();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const { aiEnabled, aiAvailable } = usePlatform();
  const pickRef = useRef<HTMLInputElement>(null);
  const onImport = pathname === "/import";

  const route = useCallback(async (files: File[]) => {
    const others: File[] = [];
    const packs: File[] = [];
    const fiveTools: File[] = [];
    let target: string | null = null;
    for (const f of files) {
      try {
        const kind = await classifyImport(f);
        if (kind === "pack") packs.push(f);
        else if (kind === "fivetools") fiveTools.push(f);
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
    if (fiveTools.length) {
      // B-238: 5etools-format homebrew goes to the 5etools-format import on Additional Content
      sendToFiveTools(fiveTools);
      toast.push(routedMessage(fiveTools.map((f) => f.name)));
      target ??= "/content";
    }
    if (others.length && aiEnabled) { stashImportFiles(others); navigate("/import"); return; }
    if (others.length) toast.push(unsupportedImportMessage(others.map((f) => f.name), aiAvailable), "error");
    // A fresh state tells an already-open Characters page to list the new character (B-24: a cold-start open lands there).
    if (target) navigate(target, { state: { imported: Date.now() } });
  }, [api, toast, navigate, aiEnabled, aiAvailable, notice]);
  // I0Ya0: the drop listeners read the render the page shows. A layout effect runs in the commit that shows the page, a
  // plain effect may run a task later (a render that overran its time slice): a file dropped in between went to the
  // listener of the render before (AI still off once the AI "+" showed).
  const latest = useRef({ route, aiAvailable });
  useLayoutEffect(() => { latest.current = { route, aiAvailable }; }, [route, aiAvailable]);

  // Files the OS opened with Borealis ("Open with" / share), drained on mount and when nudged: Tauri queues them
  // (macOS, iOS: `content:open-file`); Android's activity parks them for the backend and fires `borealis-open-file`.
  useEffect(() => {
    const drain = async () => {
      const taken = await api.takeOpenedFiles().catch(() => []);
      // a file the OS handed over that was not opened says why (H5wn8), it is never dropped silently
      taken.forEach((o) => { if (o.refused) toast.push(o.refused, "error"); });
      const opened = taken.filter((o) => !o.refused);
      if (opened.length) void route(opened.map((o) => base64ToFile(o.fileName, o.contentBase64)));
      // `.borealis` bundles are kept as files by the backend; ask for them after the other files were taken.
      await openOsBundles(api);
    };
    void drain();
    const off = api.subscribe("content:open-file", () => void drain());
    const onOpen = () => void drain();
    window.addEventListener("borealis-open-file", onOpen);
    return () => { off(); window.removeEventListener("borealis-open-file", onOpen); };
  }, [api, route, toast]);

  // Files dropped from the OS on the desktop app: the backend handles the native drop and sends tokens and names only.
  useEffect(() => api.subscribe("files:dropped", (drop) => {
    void (async () => {
      if (drop.rejected.length) toast.push(unsupportedImportMessage(drop.rejected, latest.current.aiAvailable), "error");
      drop.bundles.forEach((b) => openBundleFile({ token: b.token, name: b.name }));
      const [folder] = drop.folders;
      if (folder && (await notice.ensure())) {
        await startImportPreview(api, { folder: folder.token }, (m) => toast.push(m, "error"));
      }
      if (!drop.files.length) return;
      try {
        const opened = await api.takePickedFiles({ purpose: "dropped-file", tokens: drop.files.map((f) => f.token) });
        await latest.current.route(opened.map((o) => base64ToFile(o.fileName, o.contentBase64)));
      } catch (e) { toast.push(errorMessage(e), "error"); }
    })();
  }), [api, toast, notice]);

  useEffect(() => {
    const over = (e: DragEvent) => { if (hasFiles(e)) e.preventDefault(); };
    const drop = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      if (onImport) return;
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length) void latest.current.route(files);
    };
    window.addEventListener("dragover", over);
    window.addEventListener("drop", drop);
    return () => { window.removeEventListener("dragover", over); window.removeEventListener("drop", drop); };
  }, [onImport]);

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
