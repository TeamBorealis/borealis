/**
 * The notes view: an Obsidian vault opened in place (GitHub #15, `src-tauri/src/vault/`). The webview names vault files
 * only by vault-relative paths (`Places/Harbor Town.md`); the backend checks every one and never lets it leave the vault.
 */

export type VaultEntryKind = "folder" | "note" | "image" | "canvas" | "file";

export interface VaultEntry {
  /** Vault-relative, `/`-separated. */
  path: string;
  name: string;
  kind: VaultEntryKind;
  size: number;
  mtimeMs: number;
  /** iOS: iCloud Drive has not downloaded this file yet; opening it starts the download. */
  cloud?: boolean;
}

/** `.obsidian/app.json`, read only (Borealis never writes under `.obsidian/`). */
export interface VaultConfig {
  newFileLocation: "root" | "current" | "folder";
  newFileFolderPath: string;
  attachmentFolderPath: string;
  useMarkdownLinks: boolean;
  newLinkFormat: "shortest" | "relative" | "absolute";
}

export interface VaultTree {
  /** The chosen vault folder; null = no vault. */
  dir: string | null;
  name: string | null;
  /** The chosen folder cannot be read right now (the setting is kept). */
  unavailable: boolean;
  /** The folder has `.obsidian/` settings (optional: any folder is a vault). */
  obsidian: boolean;
  config: VaultConfig;
  /** Depth-first; in each folder its folders first, then files, by name. */
  entries: VaultEntry[];
  truncated: boolean;
}

export interface VaultNote {
  path: string;
  /** As on disk after any byte order mark (line endings as in the file). */
  text: string;
  /** Pass back to `vaultSave` as `baseHash`. */
  hash: string;
  mtimeMs: number;
  lineEnding: "lf" | "crlf";
  bom: boolean;
  /** Why the file opens read-only (not UTF-8, a canvas, not a note), or null. */
  readOnly: string | null;
}

export interface VaultSave {
  /** `conflict`: changed on disk since it was loaded (see `disk`); `missing`: deleted on disk. Nothing is written then. */
  status: "saved" | "conflict" | "missing";
  hash: string | null;
  mtimeMs: number | null;
  disk: VaultNote | null;
}

export interface VaultLine {
  /** 1-based. */
  line: number;
  text: string;
}

export interface VaultBacklink {
  path: string;
  count: number;
  lines: VaultLine[];
}

export interface VaultTag {
  tag: string;
  count: number;
}

export interface VaultSearchHit {
  path: string;
  nameMatch: boolean;
  lines: VaultLine[];
}

export interface VaultLineChange {
  line: number;
  before: string;
  after: string;
}

export interface VaultRenamePlan {
  from: string;
  to: string;
  files: { path: string; changes: VaultLineChange[] }[];
}

export interface VaultRenameDone {
  to: string;
  updated: string[];
  skipped: string[];
}

/** `trash-unavailable`: nothing was deleted; ask before deleting for good. */
export type VaultDeleteStatus = "trashed" | "deleted" | "trash-unavailable";

export interface VaultAttachment {
  path: string;
  /** The link to insert, written the way `.obsidian/app.json` says. */
  link: string;
}

/** `app:lifecycle` (phones): the app went to the background or came back. Phones have no vault watcher, so the notes view
 * saves when the app leaves and reads the vault again when it returns. */
export interface AppLifecycle {
  foreground: boolean;
}

/** `vault:changed`: changes other apps made. */
export interface VaultChange {
  paths: string[];
  /** Too many to list: re-read everything. */
  all: boolean;
  /** `.obsidian/app.json` changed. */
  config: boolean;
}
