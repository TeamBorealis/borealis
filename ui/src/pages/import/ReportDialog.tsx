import { useEffect, useRef, useState } from "react";
import type { ImportDraft, ImportReportPreview } from "../../api/contract";
import { Alert, Button, Check, Modal, TextArea } from "../../components";
import { Expander } from "../../components/controls";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";

/** What the report is about: a draft under review or an imported pack. */
export type ReportTarget = { draftId: string } | { packId: string };

const NOTE_DELAY_MS = 400;
const kb = (n: number) => `${Math.max(1, Math.round(n / 1024))} KB`;

/** BR1: the review's self-checks found something (level test, failed or doubtful elements). */
export function selfChecksFailed(draft: ImportDraft): boolean {
  return draft.sweep.errors.length > 0 || draft.elements.some((e) => e.status === "failed" || e.status === "needs_review");
}

type Phase = { kind: "edit" } | { kind: "sending" } | { kind: "sent"; id: string } | { kind: "failed"; message: string };

/**
 * Opt-in import bug report (BR1-BR3): shows the exact payload, a note and the separate source opt-in.
 * Nothing leaves the device until Send; Send posts the payload shown here, never a rebuilt one.
 */
export function ReportDialog({ target, flagged = [], onClose }: { target: ReportTarget; flagged?: string[]; onClose: () => void }) {
  const api = useApi();
  const [note, setNote] = useState("");
  const [includeSource, setIncludeSource] = useState(false);
  const [preview, setPreview] = useState<ImportReportPreview | null>(null);
  const [stale, setStale] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "edit" });
  const flaggedKey = flagged.join("\n");
  const draftId = "draftId" in target ? target.draftId : undefined;
  const packId = "packId" in target ? target.packId : undefined;
  const loaded = useRef(false);

  useEffect(() => {
    let live = true;
    setStale(true);
    const t = setTimeout(() => {
      const args = { ...(draftId ? { draftId } : { packId }), note, includeSource, flagged: flaggedKey ? flaggedKey.split("\n") : [] };
      api.importReportPreview(args)
        .then((p) => { if (live) { loaded.current = true; setPreview(p); setLoadError(null); setStale(false); } })
        .catch((e) => { if (live) { setLoadError(errorMessage(e)); setStale(false); } });
    }, loaded.current ? NOTE_DELAY_MS : 0);
    return () => { live = false; clearTimeout(t); };
  }, [api, draftId, packId, note, includeSource, flaggedKey]);

  const send = async () => {
    if (!preview) return;
    setPhase({ kind: "sending" });
    try { setPhase({ kind: "sent", id: (await api.importReportSend({ previewId: preview.previewId })).id }); }
    catch (e) { setPhase({ kind: "failed", message: errorMessage(e) }); }
  };

  const sent = phase.kind === "sent";
  const tooLarge = !!preview?.sourceBytes && preview.sourceBytes > preview.sourceLimit;
  const footer = sent
    ? <Button variant="primary" onClick={onClose}>Close</Button>
    : <>
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="primary" icon="send" disabled={!preview || stale || !!loadError || phase.kind === "sending"} onClick={() => void send()}>
          {phase.kind === "sending" ? "Sending..." : "Send"}
        </Button>
      </>;

  return (
    <Modal title="Send a report" wide className="imp-report-dlg" onClose={onClose} footer={footer}>
      <div className="stack">
        {sent ? (
          <Alert tone="success" role="status">Report sent. Thank you, it helps fix the importer{phase.id ? ` (reference ${phase.id})` : ""}.</Alert>
        ) : (
          <>
            <p className="imp-report-lead">Nothing is sent unless you press Send. The report holds the structure of this import (element types, names, rules and the problems found), not the descriptions from your source.</p>
            {flagged.length > 0 && <Alert tone="info">Marked as wrong: {flagged.join(", ")}</Alert>}
            <TextArea label="What went wrong? (optional)" rows={3} maxLength={2000} value={note} onChange={(e) => setNote(e.target.value)} placeholder="e.g. the level 6 feature is missing" />
            <Check label="Include the source file" checked={includeSource} disabled={!preview?.sourceAvailable || tooLarge}
              onChange={(e) => setIncludeSource(e.target.checked)}
              description={!preview?.sourceAvailable ? "Only available before the import is added to your library."
                : tooLarge ? `The source is too large to attach (${kb(preview.sourceBytes ?? 0)}).`
                : "Only include it if you are allowed to share it. It may be someone else's copyrighted material; it is used only to fix the importer and is never published."} />
            {includeSource && <Alert tone="warning">The full text of your source will be sent with this report.</Alert>}
            {loadError && <Alert tone="danger">{loadError}</Alert>}
            {phase.kind === "failed" && <Alert tone="danger" role="alert">{phase.message}</Alert>}
            <Expander title="Report contents" defaultOpen={false} headerExtra={<span>{preview ? kb(preview.bytes) : "..."}</span>}>
              {preview
                ? <pre className="mono imp-report-json" aria-label="Report contents" tabIndex={0}>{JSON.stringify(preview.payload, null, 2)}</pre>
                : <p className="muted">Preparing the report...</p>}
              {preview && <p className="muted small">Sent to {preview.endpoint}</p>}
            </Expander>
          </>
        )}
      </div>
    </Modal>
  );
}
