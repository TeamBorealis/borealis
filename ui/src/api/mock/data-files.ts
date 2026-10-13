import type { AppSettings, ClearCachesResult, ContentUpdateStatus, DataDirs, SecondaryContentResult } from "../types";
import { DEFAULT_SETTINGS } from "./default-settings";
import { apiError } from "./errors";
import type { MockPicks } from "./picks";

const DEFAULT_DOCS = "/home/me/.local/share/borealis";
const SYSTEM = /^\/(bin|sbin|usr|etc|dev|proc|sys|boot|lib|opt|System|Library|Applications)(\/|$)/;

/** Mock of Settings > DATA FILES: token-only folders, secondary packs, caches, startup update. */
export class MockDataFiles {
  private secondaryPacks: string[] = [];
  private cacheEntries = 3;
  readonly status: ContentUpdateStatus = {
    state: "done", message: "Your content files are up to date.", updated: 0, failed: [], checkedAt: "2026-10-11T09:00:00Z",
  };

  constructor(private readonly picks: MockPicks) {}

  dirs(s: AppSettings): DataDirs {
    const documentsDir = s.documentsDir ?? DEFAULT_DOCS;
    return {
      documentsDir, defaultDocumentsDir: DEFAULT_DOCS, isDefault: s.documentsDir === null,
      charactersDir: `${documentsDir}/characters`, secondaryContentDir: s.secondaryContentDir,
    };
  }

  /** Returns the new settings for a `documents-dir` token (or the default when absent). */
  setDocuments(s: AppSettings, token?: string): AppSettings {
    if (token === undefined) return { ...s, documentsDir: null };
    return { ...s, documentsDir: this.checked(this.picks.take(token, "documents-dir")) };
  }

  setSecondary(s: AppSettings, token: string): { settings: AppSettings; result: SecondaryContentResult } {
    const dir = this.checked(this.picks.take(token, "secondary-content-dir"));
    const settings = { ...s, secondaryContentDir: dir };
    const removed = this.secondaryPacks;
    this.secondaryPacks = ["sec-homebrew-feats", "sec-my-subclasses"];
    return { settings, result: { dirs: this.dirs(settings), report: {
      installed: this.secondaryPacks, removed: removed.filter((p) => !this.secondaryPacks.includes(p)),
      skipped: [{ file: "drafts/broken.xml", reason: "Cannot convert this file: unexpected end of file" }],
    } } };
  }

  unlink(s: AppSettings): { settings: AppSettings; result: SecondaryContentResult } {
    const settings = { ...s, secondaryContentDir: null };
    const removed = this.secondaryPacks;
    this.secondaryPacks = [];
    return { settings, result: { dirs: this.dirs(settings), report: { installed: [], removed, skipped: [] } } };
  }

  /** Classic builder Restore Defaults: Borealis-only fields (AI, default ruleset, autosave) are kept. */
  restore(s: AppSettings): AppSettings {
    this.secondaryPacks = [];
    return { ...DEFAULT_SETTINGS, ai: s.ai, mobile: s.mobile, defaultRuleset: s.defaultRuleset, autosave: s.autosave };
  }

  clearCaches(): ClearCachesResult {
    const n = this.cacheEntries;
    this.cacheEntries = 0;
    return { tempFoldersRemoved: n > 0 ? 1 : 0, aiTempFoldersRemoved: n > 0 ? 2 : 0, indexesReset: n > 0 ? 1 : 0 };
  }

  private checked(path: string): string {
    if (path === "/" || SYSTEM.test(path)) throw apiError("invalid_argument", "A system folder cannot be used.");
    return path;
  }
}
