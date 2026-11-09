import { useEffect, useRef, useState } from "react";
import type { ContentProgress, FiveToolsImportReport, FiveToolsSource, PickedPath } from "../../api/contract";
import { Button, Check, ProgressBar, TextField, useImportNotice } from "../../components";
import { filesToInputs } from "../../lib/files";
import { useApi } from "../../state/ApiContext";
import { usePlatform } from "../../state/platform";
import { errorMessage } from "../../state/useAsync";
import { onFiveToolsFiles, takeFiveToolsFiles } from "../../state/fiveToolsInbox";
import { FiveToolsReport } from "./FiveToolsReport";
import { FiveToolsSources } from "./FiveToolsSources";
import { FiveToolsUrlSection } from "./FiveToolsUrlSection";
import { Section } from "./Section";

/** The phone route (no folder picker there); desktop offers it next to the folder scan. */
export const ZIP_BUTTON = "Choose a .zip of a 5etools-format data/ folder";
/** Single homebrew files (`_meta` + entity arrays), several at once (B-238). The backend names this button in the
 *  message it gives when a homebrew file reaches the custom content import. */
export const HOMEBREW_BUTTON = "Choose 5etools-format homebrew .json files";
/** Android's chooser filters by MIME type; a downloaded .json may have none, so phones also accept untyped files. */
const HOMEBREW_ACCEPT = ".json,application/json";
const UNTYPED_FILE = "application/octet-stream";

/** Convert the user's own 5etools `data/` folder, picked as a folder (desktop), handed over as a
 *  `.zip` of it, or downloaded from an address the user pastes (every platform), or single
 *  5etools-format homebrew files (every platform; the other importers send them here too). No
 *  default address exists. */
export function FiveToolsTab() {
  const api = useApi();
  const notice = useImportNotice();
  const { mobile } = usePlatform();
  const zipRef = useRef<HTMLInputElement>(null);
  const brewRef = useRef<HTMLInputElement>(null);
  const top = useRef<HTMLDivElement>(null);
  /** True once the notice was accepted for the chooser just opened, so choosing a file does not ask twice. */
  const acknowledged = useRef(false);
  const [path, setPath] = useState("");
  /** Token for the folder confirmed in the backend's native dialog (raw paths are never sent). */
  const [folder, setFolder] = useState<string | null>(null);
  const [sources, setSources] = useState<FiveToolsSource[] | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const [fluff, setFluff] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ContentProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [report, setReport] = useState<FiveToolsImportReport | null>(null);
  /** Counts finished imports, so the address list picks up a newly remembered address. */
  const [imports, setImports] = useState(0);
  useEffect(() => api.subscribe("import5etools:progress", setProgress), [api]);

  /** Scan the folder behind a `5etools-folder` token (a picked folder or an unpacked zip). */
  const scanPick = async (pick: PickedPath) => {
    setPath(pick.path);
    setFolder(pick.token);
    const r = await api.scan5etoolsFolder({ folder: pick.token });
    setSources(r.sources);
    setPicked(r.sources.map((s) => s.abbr));
  };
  const scan = async () => {
    if (!(await notice.ensure())) return;
    setError(null); setReport(null);
    try {
      const [pick] = await api.pickPath({ purpose: "5etools-folder", ...(path ? { startDir: path } : {}) });
      if (pick) await scanPick(pick);
    } catch (e) { setSources(null); setError(errorMessage(e)); }
  };
  const chooseZip = async () => { if (!(await notice.ensure())) return; acknowledged.current = true; zipRef.current?.click(); };
  const onZip = async (list: FileList | null) => {
    const [file] = Array.from(list ?? []);
    if (zipRef.current) zipRef.current.value = "";
    const ok = acknowledged.current || (!!file && await notice.ensure());
    acknowledged.current = false;
    if (!ok || !file) return;
    setError(null); setReport(null); setSources(null); setPath(file.name);
    try {
      const inputs = await filesToInputs([file]);
      for (const input of inputs) await scanPick(await api.stage5etoolsZip({ file: input }));
    } catch (e) { setSources(null); setError(errorMessage(e)); }
  };
  /** Stage homebrew files (from the chooser or sent by another importer) and scan them. */
  const stageFiles = async (files: File[]) => {
    setError(null); setReport(null); setSources(null);
    setPath(files.length === 1 ? files[0]!.name : `${files[0]!.name} and ${files.length - 1} more`);
    try { await scanPick(await api.stage5etoolsFiles({ files: await filesToInputs(files) })); }
    catch (e) { setSources(null); setError(errorMessage(e)); }
  };
  const chooseBrew = async () => { if (!(await notice.ensure())) return; acknowledged.current = true; brewRef.current?.click(); };
  const onBrew = async (list: FileList | null) => {
    const files = Array.from(list ?? []);
    if (brewRef.current) brewRef.current.value = "";
    const ok = acknowledged.current || (files.length > 0 && await notice.ensure());
    acknowledged.current = false;
    if (ok && files.length) await stageFiles(files);
  };
  // Homebrew files dropped, opened or chosen in another importer arrive here.
  const stageRef = useRef(stageFiles);
  stageRef.current = stageFiles;
  useEffect(() => {
    const take = async () => {
      const files = takeFiveToolsFiles();
      if (!files.length || !(await notice.ensure())) return;
      top.current?.scrollIntoView?.({ block: "start" });
      await stageRef.current(files);
    };
    void take();
    return onFiveToolsFiles(() => void take());
  }, [notice]);
  const run = async () => {
    setRunning(true); setError(null); setProgress(null);
    try { setReport(await api.import5etools({ request: { folder: folder ?? "", sources: picked, includeFluff: fluff } })); setImports((n) => n + 1); }
    catch (e) { setError(errorMessage(e)); }
    finally { setRunning(false); }
  };
  return (
    <>
      <Section title="5etools-format import" guide="import-5etools-data">
        <div className="stack" ref={top}>
          <p className="muted">{mobile
            ? <>Choose a .zip of a 5etools-format <code>data/</code> folder.</>
            : <>Choose a local folder of 5etools-format data, or a .zip of it.</>} Single homebrew .json files work too, several at once. Files are converted on this device without downloading anything, and converted packs are for your personal use only.</p>
          <TextField label={mobile ? "Data folder .zip" : "Data folder"} value={path} readOnly placeholder={mobile ? "No file chosen" : "No folder chosen"} error={error} />
          <div className="v4-content-cmds">
            {!mobile && <Button aria-label="Scan folder…" variant="primary" onClick={() => void scan()}>Scan folder...</Button>}
            <Button icon="upload" variant={mobile ? "primary" : undefined} onClick={() => void chooseZip()}>{ZIP_BUTTON}</Button>
            <Button icon="upload" onClick={() => void chooseBrew()}>{HOMEBREW_BUTTON}</Button>
          </div>
          <input ref={zipRef} type="file" accept=".zip,application/zip" hidden aria-label="Choose a .zip of 5etools-format data" onChange={(e) => void onZip(e.target.files)} />
          <input ref={brewRef} type="file" multiple accept={mobile ? `${HOMEBREW_ACCEPT},${UNTYPED_FILE}` : HOMEBREW_ACCEPT} hidden aria-label="Choose 5etools-format homebrew files" onChange={(e) => void onBrew(e.target.files)} />
        </div>
      </Section>
      <FiveToolsUrlSection
        onStaged={async (pick) => { setReport(null); setSources(null); await scanPick(pick); }}
        onReport={(r) => { setSources(null); setReport(r); }}
        refresh={imports}
      />
      {sources && (
        <Section title="Source books found" actions={<Button variant="primary" disabled={running || picked.length === 0} onClick={() => void run()}>Import {picked.length} book(s)</Button>}>
          <div className="stack">
            <FiveToolsSources sources={sources} picked={picked} onPicked={setPicked} />
            <Check label="Include fluff (descriptions and lore text)" checked={fluff} onChange={(e) => setFluff(e.target.checked)} />
            {running && progress && <div><ProgressBar label="Import progress" value={progress.done} max={Math.max(1, progress.total)} /><span className="small muted" role="status">{progress.message}</span></div>}
          </div>
        </Section>
      )}
      {report && (
        <Section title="Import report">
          <FiveToolsReport report={report} />
        </Section>
      )}
    </>
  );
}
