import type { MigrationReport } from "../../api/contract";
import { Alert, Button, Modal } from "../../components";
import { Expander } from "../../components/controls";
import { ImportIssues } from "./ImportIssues";

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** "12 elements converted, 1 skipped; 0 errors, 2 warnings, 1 note." */
export function reportSummary(r: MigrationReport): string {
  const count = (s: string) => r.issues.filter((d) => d.severity === s).length;
  const skipped = r.skipped ? `, ${r.skipped} skipped` : "";
  return `${plural(r.converted, "element")} converted${skipped}; ${plural(count("error"), "error")}, ${plural(count("warning"), "warning")}, ${plural(count("info"), "note")}.`;
}

/** One report: its counts and its issues (grouped by severity, then by source file). */
function ReportBody({ report }: { report: MigrationReport }) {
  return (
    <>
      <p className="muted" role="status">{reportSummary(report)}</p>
      <ImportIssues issues={report.issues} />
    </>
  );
}

/**
 * The conversion report of an install or of an index update (one report per converted pack), in the import preview's
 * report dialog style: each issue names its source file, element (type and id), line and code.
 */
export function ImportReportDialog({ title, reports, onClose }: { title: string; reports: readonly MigrationReport[]; onClose: () => void }) {
  const single = reports.length === 1 ? reports[0] : undefined;
  return (
    <Modal title={title} wide className="aur-rep-dlg" onClose={onClose} footer={<Button variant="primary" onClick={onClose}>Close</Button>}>
      <div className="stack">
        {reports.length === 0 && <Alert tone="success">Nothing was converted.</Alert>}
        {single ? <ReportBody report={single} /> : reports.map((r) => (
          <Expander key={r.fileName} title={`${r.fileName} (${plural(r.issues.length, "issue")})`} defaultOpen={false}>
            <div className="stack"><ReportBody report={r} /></div>
          </Expander>
        ))}
      </div>
    </Modal>
  );
}
