import type { ImportDraft, ImportReportPreview, ImportReportRequest, ImportReportSent } from "../types";
import { apiError } from "./errors";

const ENDPOINT = "https://borealisbuilder.com/api/import-report";
const SOURCE_LIMIT = 1_000_000;
const MOCK_LOGS = "INFO borealis_lib::app_logs: Borealis started\nWARN borealis_lib::content_index: index https://example.test/a.index: refused";
const MOCK_SOURCE = "Runebinder\nHit Die d8. Saving throws: Intelligence, Wisdom.";

/** Mirrors the backend outbox: previews are structure-only payloads; `send` posts nothing, it records. */
export class MockImportReports {
  readonly sent: Record<string, unknown>[] = [];
  /** Tests set this to make the next sends fail like an unreachable server. */
  failSend = false;
  private previews = new Map<string, Record<string, unknown>>();
  private seq = 0;

  preview(req: ImportReportRequest, draft: (id: string) => ImportDraft): ImportReportPreview {
    if (!!req.draftId === !!req.packId) throw apiError("invalid_argument", "Report on either an import draft or an imported pack.");
    const d = req.draftId ? draft(req.draftId) : null;
    const elements = (d?.elements ?? []).map((e) => {
      const { description, ...structure } = e.json as Record<string, unknown> & { description?: string };
      return { id: e.id, type: e.type, name: e.name, status: e.status, issues: e.validationIssues, description_chars: description?.length ?? 0, structure };
    });
    const failed = d ? d.sweep.errors.length > 0 || d.elements.some((e) => e.status === "failed" || e.status === "needs_review") : false;
    const flagged = req.flagged ?? [];
    const codes = [
      ...(d?.sweep.errors.map((m) => ({ code: "engine", pattern: m, count: 1 })) ?? []),
      ...elements.filter((e) => e.status === "needs_review" || e.status === "failed").map((e) => ({ code: e.status, pattern: e.type, count: 1 })),
      ...flagged.map(() => ({ code: "user_flagged", pattern: "element", count: 1 })),
    ];
    const payload: Record<string, unknown> = {
      format: "borealis-import-bug-report/1",
      app: { version: "0.0.0-mock", os: "mock", arch: "mock" },
      edition: d?.detected.edition ?? null,
      origin: d ? "draft" : "pack",
      summary: { elements: elements.length, diagnostics: d?.sweep.errors.length ?? 0, flagged: flagged.length },
      codes,
      report: { format: "borealis-import-report/1", entries: [] },
      diagnostics: (d?.sweep.errors ?? []).map((message) => ({ level: null, element: null, message })),
      elements,
      flagged,
      note: (req.note ?? "").trim(),
      source: d && req.includeSource ? { name: "pasted text", kind: "text", chars: MOCK_SOURCE.length, text: MOCK_SOURCE } : null,
      ...(req.includeLogs ? { logs: { lines: MOCK_LOGS.split("\n").length, text: MOCK_LOGS } } : {}),
    };
    const previewId = `preview-${++this.seq}`;
    this.previews.set(previewId, payload);
    return {
      previewId, payload, bytes: JSON.stringify(payload).length, selfChecksFailed: failed || codes.length > flagged.length,
      sourceAvailable: !!d, sourceBytes: d ? MOCK_SOURCE.length : null, sourceLimit: SOURCE_LIMIT, logsAvailable: true, endpoint: ENDPOINT,
    };
  }

  send(previewId: string): ImportReportSent {
    const payload = this.previews.get(previewId);
    if (!payload) throw apiError("not_found", "This report preview has expired; open it again.");
    if (this.failSend) throw apiError("io_error", "The report could not be sent: the server could not be reached");
    this.previews.delete(previewId);
    this.sent.push(payload);
    return { id: `report-${this.sent.length}` };
  }
}
