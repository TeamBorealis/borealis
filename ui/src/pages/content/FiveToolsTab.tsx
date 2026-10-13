import { useEffect, useState } from "react";
import type { ContentProgress, FiveToolsImportReport, FiveToolsSource } from "../../api/contract";
import { Button, Check, ProgressBar, TextField, useImportNotice } from "../../components";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";
import { FiveToolsReport } from "./FiveToolsReport";
import { FiveToolsSources } from "./FiveToolsSources";
import { Section } from "./Section";

/** Convert the user's own local 5etools `data/` folder. Nothing is downloaded and no default URL exists. */
export function FiveToolsTab() {
  const api = useApi();
  const notice = useImportNotice();
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
  useEffect(() => api.subscribe("import5etools:progress", setProgress), [api]);

  const scan = async () => {
    if (!(await notice.ensure())) return;
    setError(null); setReport(null);
    try {
      const [pick] = await api.pickPath({ purpose: "5etools-folder", ...(path ? { startDir: path } : {}) });
      if (!pick) return;
      setPath(pick.path);
      setFolder(pick.token);
      const r = await api.scan5etoolsFolder({ folder: pick.token });
      setSources(r.sources);
      setPicked(r.sources.map((s) => s.abbr));
    } catch (e) { setSources(null); setError(errorMessage(e)); }
  };
  const run = async () => {
    setRunning(true); setError(null); setProgress(null);
    try { setReport(await api.import5etools({ request: { folder: folder ?? "", sources: picked, includeFluff: fluff } })); }
    catch (e) { setError(errorMessage(e)); }
    finally { setRunning(false); }
  };
  return (
    <>
      <Section title="5etools import">
        <div className="stack">
          <p className="muted">Choose your own local 5etools <code>data/</code> folder. Files are read from disk and converted on this device; nothing is downloaded, and converted packs are for your personal use only.</p>
          <TextField label="Data folder" value={path} readOnly placeholder="No folder chosen" error={error} />
          <div><Button aria-label="Scan folder…" variant="primary" onClick={() => void scan()}>Scan folder...</Button></div>
        </div>
      </Section>
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
