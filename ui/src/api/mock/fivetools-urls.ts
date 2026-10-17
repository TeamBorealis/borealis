import type { FiveToolsImportReport, FiveToolsUrl, FiveToolsUrlUpdate } from "../types";
import { apiError } from "./errors";

const key = (url: string) => url.trim().replace(/\/+$/, "");

/** Mirrors `content_fivetools_url.rs`: addresses 5etools data was imported from, and their Update. */
export class MockFiveToolsUrls {
  /** Test hook: the revision the mock "remote" is at; change it to make Update re-import. */
  revision = "a1b2c3d4e5f6";
  /** Test hook: when set, the next download fails with this message. */
  failWith: string | null = null;
  private records: FiveToolsUrl[] = [];
  private staged = new Map<string, string>();

  /** `stage_5etools_url` checks: https only, and a mock "download". */
  check(url: string): string {
    const k = key(url);
    if (!/^https:\/\/[^/\s]+/.test(k)) throw apiError("invalid_argument", "5etools addresses must start with https://");
    if (this.failWith) { const why = this.failWith; this.failWith = null; throw apiError("invalid_argument", why); }
    return k;
  }

  /** Remember which pick token came from which address. */
  stage(token: string, url: string): void { this.staged.set(token, key(url)); }

  /** After `import_5etools` on a staged token: remember the address and the chosen books. */
  remember(token: string, sources: string[], report: FiveToolsImportReport): void {
    const url = this.staged.get(token);
    if (!url) return;
    const now = new Date().toISOString();
    const record: FiveToolsUrl = { url, revision: this.revision, sources: [...sources], packs: report.packs.map((p) => p.id), lastChecked: now, lastImported: now, problem: null };
    this.records = [...this.records.filter((r) => r.url !== url), record];
  }

  list(): FiveToolsUrl[] { return this.records.map((r) => ({ ...r })); }

  remove(url: string): FiveToolsUrl[] { this.records = this.records.filter((r) => r.url !== key(url)); return this.list(); }

  update(url: string, reimport: (sources: string[]) => FiveToolsImportReport): FiveToolsUrlUpdate {
    const record = this.records.find((r) => r.url === key(url));
    if (!record) throw apiError("invalid_argument", "That address has not been imported yet.");
    const now = new Date().toISOString();
    const save = (next: FiveToolsUrl) => { this.records = this.records.map((r) => (r.url === next.url ? next : r)); };
    if (this.failWith) {
      const why = this.failWith; this.failWith = null;
      save({ ...record, lastChecked: now, problem: why });
      throw apiError("invalid_argument", why);
    }
    if (record.revision === this.revision) {
      save({ ...record, lastChecked: now, problem: null });
      return { upToDate: true, report: null, urls: this.list() };
    }
    const report = reimport(record.sources);
    save({ ...record, revision: this.revision, packs: report.packs.map((p) => p.id), lastChecked: now, lastImported: now, problem: null });
    return { upToDate: false, report, urls: this.list() };
  }
}
