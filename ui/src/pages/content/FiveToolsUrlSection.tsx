import { useEffect, useState } from "react";
import type { ContentProgress, FiveToolsImportReport, FiveToolsUrl, PickedPath } from "../../api/contract";
import { Button, ProgressBar, useConfirm, useImportNotice, useToast } from "../../components";
import { useApi } from "../../state/ApiContext";
import { errorMessage, useAsync } from "../../state/useAsync";
import { sourceConsentText } from "./IndexesTab";
import { setStatus as setStatusBar } from "../../state/status";
import { UP_TO_DATE } from "./indexUpdateText";
import { Section } from "./Section";

export const URL_IMPORT_BUTTON = "Import";

const hostOf = (url: string): string | null => { try { return new URL(url).host || null; } catch { return null; } };
const timeOf = (iso: string) => new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

/** What the status line says after Update. */
export function describeUrlUpdate(upToDate: boolean, report: FiveToolsImportReport | null): string {
  if (upToDate || !report) return UP_TO_DATE;
  return `The address changed: ${report.converted} element(s) re-imported into ${report.packs.length} pack(s).`;
}

interface Props {
  /** The downloaded folder, to scan and offer its books (same flow as a chosen folder). */
  onStaged: (pick: PickedPath) => Promise<void>;
  /** A re-import made by Update. */
  onReport: (report: FiveToolsImportReport) => void;
  /** Changes after every finished import, so the list shows newly remembered addresses. */
  refresh: number;
}

/** 5etools import from an address the user pastes (GitHub repository or data/ folder), with Update.
 *  No address ships with the app; every platform, phones included, downloads over https only. */
export function FiveToolsUrlSection({ onStaged, onReport, refresh }: Props) {
  const api = useApi();
  const toast = useToast();
  const confirm = useConfirm();
  const notice = useImportNotice();
  const urls = useAsync(() => api.list5etoolsUrls(), [api, refresh]);
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  const [progress, setProgress] = useState<ContentProgress | null>(null);
  useEffect(() => api.subscribe("import5etools:progress", setProgress), [api]);

  const known = (target: string) => (urls.data ?? []).some((u) => u.url === target.replace(/\/+$/, ""));
  const download = async () => {
    const target = url.trim();
    if (!(await notice.ensure())) return;
    const host = hostOf(target);
    if (host && !known(target)) {
      const r = await confirm({ title: "Import from an address", body: sourceConsentText(host), confirmLabel: "Download" });
      if (r !== "confirm") return;
    }
    setBusy("download"); setError(null); setStatus(""); setProgress(null);
    try { await onStaged(await api.stage5etoolsUrl({ url: target })); }
    catch (e) { setError(errorMessage(e)); }
    finally { setBusy(null); }
  };
  const update = async (target: FiveToolsUrl) => {
    setBusy(target.url); setStatus("Checking..."); setProgress(null);
    try {
      const r = await api.update5etoolsUrl({ url: target.url });
      const text = describeUrlUpdate(r.upToDate, r.report);
      setStatus(text);
      setStatusBar(text);
      if (r.report) onReport(r.report);
    } catch (e) { setStatus(errorMessage(e)); setStatusBar(errorMessage(e)); }
    finally { setBusy(null); urls.reload(); }
  };
  const remove = (target: FiveToolsUrl) => void api.remove5etoolsUrl({ url: target.url }).then(urls.reload).catch((e) => toast.push(errorMessage(e), "error"));

  return (
    <Section title="Import from an address">
      <p>Paste the address of a GitHub repository that holds a 5etools <code>data/</code> folder, or the address of the folder itself. Borealis does not come with an address; only use sources you trust and content you are entitled to use.</p>
      <form className="v4-urlrow" onSubmit={(e) => { e.preventDefault(); void download(); }}>
        <label className="v4-fieldhead" htmlFor="fivetools-url">5etools data address</label>
        <div className="v4-urlbox">
          <input id="fivetools-url" className="v4-urlinput" type="url" placeholder="https://github.com/<owner>/<repo>" aria-label="5etools data address" value={url} onChange={(e) => setUrl(e.target.value)} aria-invalid={error ? true : undefined} />
          <Button type="submit" variant="primary" icon="download" className="v4-dl" disabled={!url.trim() || busy !== null}>{URL_IMPORT_BUTTON}</Button>
        </div>
        {error && <span className="v4-urlerr" role="alert">{error}</span>}
      </form>
      {busy && progress && progress.phase === "downloading" && <div style={{ marginTop: "var(--sp-3)" }}><ProgressBar label="Download progress" value={progress.done} max={Math.max(1, progress.total)} /><span className="small muted" role="status">{progress.message}</span></div>}
      {urls.data && urls.data.length > 0 && (
        <ul className="list" aria-label="5etools addresses">
          {urls.data.map((u) => (
            <li key={u.url}><div className="inv-row" style={{ padding: "var(--sp-3)" }}>
              <span style={{ flex: 1, minWidth: 0, overflowWrap: "anywhere" }}><strong>{u.url}</strong><span className="muted small" style={{ display: "block" }}>{u.sources.length} book(s) · revision {u.revision}{u.lastChecked ? ` · Last checked at ${timeOf(u.lastChecked)}` : ""}</span>{u.problem && <span className="v4-index-problem small" role="note">{u.problem}</span>}</span>
              <Button small onClick={() => void update(u)} disabled={busy !== null} aria-label={`Update ${u.url}`}>Update</Button>
              <Button small variant="danger" onClick={() => remove(u)} disabled={busy !== null} aria-label={`Remove ${u.url}`}>Remove</Button>
            </div></li>
          ))}
        </ul>
      )}
      {status && <p className="v4-status" role="status">{status}</p>}
    </Section>
  );
}
