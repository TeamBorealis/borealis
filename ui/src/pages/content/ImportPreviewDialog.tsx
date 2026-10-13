import { useNavigate } from "react-router-dom";
import type { ImportPreview, PreviewDiagnostic } from "../../api/contract";
import { Alert, Button, Modal, useToast } from "../../components";
import { Expander } from "../../components/controls";
import { useApi } from "../../state/ApiContext";
import { closeImportPreview, dismissImportPreview, setImportBusy, useImportPreviewState } from "../../state/importPreview";
import { errorMessage } from "../../state/useAsync";

const SEVERITIES = [["error", "Errors"], ["warning", "Warnings"], ["info", "Notes"]] as const;
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

function Diagnostics({ list, severity, label }: { list: PreviewDiagnostic[]; severity: string; label: string }) {
  const rows = list.filter((d) => d.severity === severity);
  if (!rows.length) return null;
  return (
    <Expander title={`${label} (${rows.length})`} defaultOpen={false}>
      <ul className="v4-impdiag" aria-label={label}>
        {rows.map((d, n) => (
          <li key={n}>{d.message}{(d.element || d.file) && <span className="muted small"> ({[d.element, d.file].filter(Boolean).join(" in ")})</span>}</li>
        ))}
      </ul>
    </Expander>
  );
}

/** What an import would add, shown before anything is installed: counts by type, grouped problems, unresolved references. */
export function ImportPreviewDialog({ preview }: { preview: ImportPreview }) {
  const api = useApi();
  const toast = useToast();
  const navigate = useNavigate();
  const { busy } = useImportPreviewState();
  const none = preview.converted === 0;
  const install = async () => {
    setImportBusy(true);
    try {
      const r = await api.installImportPreview({ previewId: preview.previewId });
      toast.push(r.pack ? `Imported "${r.pack.name}" (${plural(r.report.converted, "element")}).` : "Nothing was imported.", r.pack ? "info" : "error");
      closeImportPreview();
      navigate("/content");
    } catch (e) { setImportBusy(false); toast.push(errorMessage(e), "error"); }
  };
  const { errors, warnings } = preview.summary;
  return (
    <Modal title="Import preview" wide className="aur-rep-dlg" onClose={() => dismissImportPreview(api)}
      footer={<><Button onClick={() => dismissImportPreview(api)}>Cancel</Button><Button variant="primary" disabled={none || busy} onClick={() => void install()}>Install</Button></>}>
      <div className="stack">
        <Alert tone={none ? "danger" : errors ? "warning" : "success"}>
          {none ? `Nothing in ${preview.packName} can be imported.` : `${plural(preview.converted, "element")} from ${plural(preview.fileCount, "file")} will be added as "${preview.packName}".`}
          {!none && preview.skipped > 0 && ` ${preview.skipped} skipped.`}
        </Alert>
        {preview.replacesExisting && !none && <Alert tone="warning">An installed pack named "{preview.packName}" will be replaced.</Alert>}
        {preview.counts.length > 0 && (
          <Expander title="Converted">
            <ul className="v4-impcounts" aria-label="Converted elements">
              {preview.counts.map((c) => <li key={c.type}><span>{c.type}</span><strong>{c.count}</strong></li>)}
            </ul>
          </Expander>
        )}
        {(errors > 0 || warnings > 0 || preview.summary.infos > 0) && (
          <p className="muted" role="status">{plural(errors, "error")}, {plural(warnings, "warning")}, {plural(preview.summary.infos, "note")}.</p>
        )}
        {SEVERITIES.map(([s, label]) => <Diagnostics key={s} list={preview.diagnostics} severity={s} label={label} />)}
        {preview.diagnosticsTruncated && <p className="muted small">Only the first problems are listed.</p>}
        {preview.unresolvedTotal > 0 && (
          <Expander title={`Unresolved references (${preview.unresolvedTotal})`} defaultOpen>
            <p>Import the content these depend on (for example the rest of your content folder), or install anyway and add it later.</p>
            <ul className="v4-impdiag" aria-label="Unresolved references">
              {preview.unresolved.map((u) => (
                <li key={u.target}>{u.target}<span className="muted small"> used by {u.referencedBy.join(", ")}{u.count > u.referencedBy.length ? ` and ${u.count - u.referencedBy.length} more` : ""}</span></li>
              ))}
            </ul>
          </Expander>
        )}
      </div>
    </Modal>
  );
}

/** Mounted once (by GlobalImport) so every entry point opens the same dialog. */
export function ImportPreviewHost() {
  const { preview } = useImportPreviewState();
  return preview ? <ImportPreviewDialog preview={preview} /> : null;
}
