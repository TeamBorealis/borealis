import { useEffect, useState } from "react";
import type { AppFolder, ContentProgress } from "../../api/contract";
import { Alert, Button, ProgressBar, useConfirm, useToast } from "../../components";
import { useApi } from "../../state/ApiContext";
import { errorMessage, useAsync } from "../../state/useAsync";
import { Section } from "./Section";

/** First-fetch consent: shown before an index is added (nothing is verified until it is fetched). */
export const sourceConsentText = (host: string) =>
  `This source is not verified by Borealis. Content will be downloaded from ${host} and converted on this device. Only add sources you trust and content you are entitled to use.`;

const hostOf = (url: string): string | null => { try { return new URL(url).host || null; } catch { return null; } };

const checkedText = (iso: string) => `Last checked at ${new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;

export function IndexesTab() {
  const api = useApi();
  const toast = useToast();
  const confirm = useConfirm();
  const indexes = useAsync(() => api.listIndexes(), [api]);
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ContentProgress | null>(null);
  useEffect(() => api.subscribe("content:progress", setProgress), [api]);
  const add = async () => {
    const target = url.trim();
    const host = hostOf(target);
    const known = (indexes.data ?? []).some((i) => i.url === target);
    if (host && !known) {
      const r = await confirm({ title: "Add content source", body: sourceConsentText(host), confirmLabel: "Add source" });
      if (r !== "confirm") return;
    }
    try { await api.addIndex({ url: target }); setUrl(""); setError(null); indexes.reload(); }
    catch (e) { setError(errorMessage(e)); }
  };
  const [status, setStatus] = useState(() => "");
  const [pending, setPending] = useState(0);
  useEffect(() => {
    void api.contentUpdateStatus().then((s) => { if (s.checkedAt) setStatus(checkedText(s.checkedAt)); setPending(s.updated); }).catch(() => undefined);
  }, [api]);
  const update = async () => {
    setStatus("Checking...");
    try {
      const r = await api.updateIndexes();
      setPending((n) => n + r.updated);
      const notes = [
        r.changed.length ? `${r.changed.length} file(s) changed since they were first downloaded.` : "",
        r.newHosts.length ? `New download source(s): ${r.newHosts.join(", ")}.` : "",
        r.failed.length ? `${r.failed.length} could not be updated.` : "",
      ].filter(Boolean).join(" ");
      const base = r.updated ? "Content files have been updated, restart the application to load them." : checkedText(new Date().toISOString());
      setStatus(notes ? `${base} ${notes}` : base);
      toast.push(r.updated ? `${r.updated} file(s) updated. Content reloaded.` : "Everything is up to date.");
      if (r.changed.length || r.newHosts.length) toast.push(notes, "warning");
      indexes.reload();
    } catch (e) { setStatus(errorMessage(e)); toast.push(errorMessage(e), "error"); }
  };
  const openFolder = (folder: AppFolder) => void api.openAppFolder({ folder }).catch((e) => toast.push(errorMessage(e), "error"));
  const clear = async () => {
    const r = await confirm({ title: "Clear downloaded content", body: "Delete all content downloaded from indexes? Your own imports are kept.", confirmLabel: "Clear", danger: true });
    if (r === "confirm") await api.clearDownloadedContent();
  };
  return (
    <>
    <Section title="Index files">
      <p>Index files list content that can be downloaded and kept up to date. Only add index files from publishers you trust.</p>
      <form className="v4-urlrow" onSubmit={(e) => { e.preventDefault(); void add(); }}>
        <label className="v4-fieldhead" htmlFor="index-url">Remote .index file URL</label>
        <div className="v4-urlbox">
          <input id="index-url" className="v4-urlinput" type="url" placeholder="https://" aria-label="Index URL" value={url} onChange={(e) => setUrl(e.target.value)} aria-invalid={error ? true : undefined} />
          <Button type="submit" variant="primary" icon="download" className="v4-dl" disabled={!url.trim()}>Fetch</Button>
        </div>
        {error && <span className="v4-urlerr" role="alert">{error}</span>}
      </form>
      {indexes.data && indexes.data.length === 0 ? <p className="muted">No index files downloaded.</p> : (
        <ul className="list" aria-label="Indexes">
          {(indexes.data ?? []).map((i) => (
            <li key={i.url}><div className="inv-row" style={{ padding: "var(--sp-3)" }}>
              <span style={{ flex: 1 }}><strong>{i.name}</strong>{!i.verified && <UnverifiedBadge />}<span className="muted small" style={{ display: "block" }}>{i.url} · {i.fileCount} files · {i.updatesAvailable} update(s)</span>{i.problem && <span className="v4-index-problem small" role="note">{i.problem}</span>}</span>
              {i.requiresNewerApp && <span className="chip warning">Needs a newer app</span>}
              <Button small variant="danger" onClick={() => void api.removeIndex({ url: i.url }).then(indexes.reload)}>Remove</Button>
            </div></li>
          ))}
        </ul>
      )}
    </Section>
    <Section title="Update content files">
      <p>Check the index files for newer content and download it. Restart the application to load updated files.</p>
      <div className="v4-content-cmds">
        <Button onClick={() => void update()}>Update</Button>
        <Button onClick={() => void api.restartApp().catch((e) => toast.push(errorMessage(e), "error"))}>Restart{pending > 0 && <span className="v4-badge" aria-label={`${pending} updated`}>{pending}</span>}</Button>
        <Button onClick={() => void clear()}>Clear</Button>
        <Button onClick={() => openFolder("content")}>Open</Button>
        <Button onClick={() => openFolder("user-content")}>Open user</Button>
      </div>
      {progress && progress.phase !== "done" && <div style={{ marginTop: "var(--sp-3)" }}><ProgressBar label="Update progress" value={progress.done} max={Math.max(1, progress.total)} /><span className="small muted" role="status">{progress.message}</span></div>}
      {progress?.phase === "done" && <Alert tone="success">{progress.message}</Alert>}
      {status && <p className="v4-status" role="status">{status}</p>}
    </Section>
    </>
  );
}

/** Small badge for an index file that is not signed by the pinned key. */
export function UnverifiedBadge() {
  return <span className="v4-unverified" title="This source is not verified by Borealis: it is not signed by a trusted key.">unverified</span>;
}
