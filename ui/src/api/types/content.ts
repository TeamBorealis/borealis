import type { ElementSummary, ElementType, Ruleset } from "./core";

export interface ItemSummary extends ElementSummary {
  category: string;
  costCp: number;
  weight: number;
  requiresAttunement: boolean;
}

export interface ItemQuery {
  category?: string;
  query?: string;
  source?: string;
  limit?: number;
  offset?: number;
}

export interface CompendiumQuery {
  query: string;
  types?: ElementType[];
  sources?: string[];
  ruleset?: Ruleset | "any";
  limit: number;
  offset: number;
}

export interface Facet {
  id: string;
  label: string;
  count: number;
}
export interface CompendiumFacets {
  types: Facet[];
  sources: Facet[];
}

export type PackKind = "bundled" | "imported" | "downloaded" | "homebrew" | "local-import";

export interface ContentPack {
  id: string;
  name: string;
  version: string;
  kind: PackKind;
  rulesets: Ruleset[];
  elementCount: number;
  enabled: boolean;
  license: string;
  attribution: string;
  sourceIds: string[];
  /** false for packs converted from the user's own third-party data (shown as "Local import - not redistributable"). */
  redistributable?: boolean;
}

export interface ContentSource {
  id: string;
  name: string;
  group: string;
  packId: string;
  elementCount: number;
  description: string;
  playtest: boolean;
}

export type ContentFormat = "json" | "xml";

export interface ImportPackRequest {
  fileName: string;
  /** File bytes, base64 encoded (works identically in browser and Tauri). */
  contentBase64: string;
  /** Auto-detected from the extension/content when omitted. */
  format?: ContentFormat;
}

export interface MigrationIssue {
  severity: "info" | "warning" | "error";
  element: string | null;
  message: string;
}

export interface MigrationReport {
  fileName: string;
  format: ContentFormat;
  converted: number;
  skipped: number;
  issues: MigrationIssue[];
}

export interface ImportPackResult {
  pack: ContentPack | null;
  report: MigrationReport;
}

export interface ContentIndex {
  url: string;
  name: string;
  lastChecked: string | null;
  fileCount: number;
  updatesAvailable: number;
  requiresNewerApp: boolean;
  /** Signed by the pinned key; false shows an "unverified" badge. */
  verified: boolean;
}

export interface UpdateIndexesResult {
  updated: number;
  failed: string[];
  reloaded: boolean;
}

export interface ContentProgress {
  phase: "checking" | "downloading" | "verifying" | "converting" | "reloading" | "done";
  done: number;
  total: number;
  message: string;
}

export interface RestrictionPreview {
  /** Picks that would be dropped if the restriction were applied. */
  affected: { slotId: string; slotTitle: string; element: string }[];
}

/* ---------- 5etools local import ---------- */

export interface FiveToolsSource {
  /** Book abbreviation as used by the data folder (e.g. "PHB", "XPHB"). */
  abbr: string;
  name: string;
  ruleset: Ruleset;
  /** Element counts by type found for this book. */
  counts: Record<string, number>;
}

export interface FiveToolsScan {
  sources: FiveToolsSource[];
}

export interface FiveToolsImportRequest {
  /** `pickPath({ purpose: "5etools-folder" })` token. */
  folder: string;
  /** Selected `abbr` values. */
  sources: string[];
  includeFluff: boolean;
}

export interface ImportNote {
  element: string;
  reason: string;
}

export interface FiveToolsImportReport {
  converted: number;
  partial: ImportNote[];
  skipped: ImportNote[];
  /** Packs installed into the user's library (kind "local-import", redistributable false). */
  packs: ContentPack[];
}
