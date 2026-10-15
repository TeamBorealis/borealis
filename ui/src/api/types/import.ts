import type { Ruleset } from "./core";

/** "Import anything". Streamed over `import:event` (`borealis://import-event`). */
export type ImportKind = "class" | "subclass" | "species" | "feat" | "spell" | "item" | "monster" | "background";
export type ImportStage = "detect" | "plan" | "generate" | "validate" | "repair" | "ready" | "error";
export type ImportElementStatus = "ok" | "repaired" | "needs_review" | "failed";

/**
 * `bytes`: contents read by the webview (drop, picker, camera; the webview never sees real paths).
 * `file`: a `pickPath({ purpose: "import-file" })` token (desktop only); the backend derives the name
 *   and type from the picked file. Limits are enforced by Rust (`IMPORT_LIMITS`).
 */
export type ImportSource =
  | { kind: "text"; text: string }
  | { kind: "file"; token: string }
  | { kind: "bytes"; name: string; mime: string; base64: string };

/** Mirrors borealis-ai's attachment guards; the UI checks before reading a file into memory. */
export const IMPORT_LIMITS = {
  maxSources: 12,
  /** Photos are downscaled before the model sees them, so the raw cap is above the 5 MB model limit. */
  maxImageBytes: 20 * 1024 * 1024,
  /** Large PDFs are split into parts under the model limit by the importer. */
  maxPdfBytes: 256 * 1024 * 1024,
  /** Word (.docx) and OpenDocument (.odt) files; their text is extracted. */
  maxDocumentBytes: 64 * 1024 * 1024,
  maxTextChars: 2_000_000,
  /** Typed notes that accompany images or PDFs in one `import_start`. */
  maxNotesChars: 4000,
} as const;

export interface ImportHint {
  kind?: ImportKind;
  edition?: Ruleset;
  parentClassId?: string;
}

/** Per-element review state: only "check" elements need the user's eyes. */
export type ImportReviewState = "ok" | "check" | "not_imported";
export interface ImportElementReview {
  state: ImportReviewState;
  /** Machine code of the leading reason: ok, accepted, pending, failed, engine, check:<code>, rule_removed, mechanic_lost, untraced, unlocated, incomplete. */
  code: string;
  /** One line in plain words ("" when ok with nothing to say). */
  reason: string;
  /** Everything else known (other reasons, model notes, automatic fixes, balance), most important first. */
  details: string[];
  /** The sheet won't do the math for some of this element (shown as "text only"). */
  textOnly: boolean;
  /** Why it is text only ("" when not). */
  textOnlyReason: string;
  /** The user's manual text-only override; null = automatic. */
  textOnlyOverride: boolean | null;
  /** The user marked it as fine (cleared automatically when the element changes). */
  accepted: boolean;
}
/** Where an element comes from in the draft's source text: 1-based inclusive line numbers into `ImportDraft.sourceLines`. */
export interface ImportSourcePassage { startLine: number; endLine: number; text: string }
/** A failed check on the whole import (not one element), e.g. headings of the source that were not imported. */
export interface ImportLevelNote { code: string; summary: string; items: string[] }
export interface ImportReviewCounts { ok: number; check: number; notImported: number; textOnly: number; accepted: number }

export interface ImportElement {
  id: string;
  /** Engine element type, e.g. class, class_feature, spellcasting, subclass, feat. */
  type: string;
  name: string;
  /** Class / subclass features: the level they arrive at (groups the review list). */
  level?: number;
  status: ImportElementStatus;
  sourceExcerpt: string;
  json: Record<string, unknown>;
  balanceFlags: string[];
  validationIssues: string[];
  review: ImportElementReview;
  sourcePassage: ImportSourcePassage | null;
}

export interface ImportEvent {
  draftId: string;
  stage: ImportStage;
  /** 0..1 */
  progress: number;
  message: string;
  element?: ImportElement;
}

export interface ImportDetected {
  kinds: ImportKind[];
  edition: Ruleset;
  names: string[];
  /** 0..1 */
  confidence: number;
}

export interface ImportParentCandidate { classId: string; name: string }

export interface ImportSweep { levelsPassed: number; errors: string[] }

export interface ImportDraft {
  draftId: string;
  stage: ImportStage;
  detected: ImportDetected;
  /** Present until the user picks (subclass with no clear parent). */
  parentChoice?: { candidates: ImportParentCandidate[] };
  parentClassId?: string;
  elements: ImportElement[];
  sweep: ImportSweep;
  suggestedPackName: string;
  suggestedPrefix: string;
  /** The source text split into lines (the transcript for images/PDFs). */
  sourceLines: string[];
  reviewCounts: ImportReviewCounts;
  /** Failed import-level source checks, one note per check (empty when none). Show as a short banner with the list. */
  importNotes: ImportLevelNote[];
}

/**
 * Text sources may be mixed with images/PDFs: when files are present the text is not the source but
 * the user's notes ("this is a 2024 Fighter subclass"), sent to the model next to the files.
 */
/** `allowUntrustedWithCodex`: the user agreed to run this import on Codex (asked once per import after `consent_required`). */
export interface ImportStartRequest { sources: ImportSource[]; hint?: ImportHint; allowUntrustedWithCodex?: boolean }
export interface ImportFinalizeRequest { packName: string; prefix: string }

/**
 * Opt-in import bug report (BR1-BR3): report on a draft under review (`draftId`) or an imported pack
 * (`packId`), exactly one. Nothing is sent until `importReportSend` with the preview's id.
 */
export interface ImportReportRequest { draftId?: string; packId?: string; note?: string; flagged?: string[]; includeSource?: boolean; includeLogs?: boolean }

/** The exact payload that `importReportSend` would post, plus what the dialog shows about it. */
export interface ImportReportPreview {
  previewId: string;
  /** Structure only: element types, ids, names, rules, setters; descriptions are removed. */
  payload: Record<string, unknown>;
  bytes: number;
  /** Errors, unlocated sections, unsupported content, engine findings or invented rule kinds. */
  selfChecksFailed: boolean;
  /** The source text can be attached (drafts only). */
  sourceAvailable: boolean;
  sourceBytes: number | null;
  sourceLimit: number;
  /** There are local log lines that can be attached (BR1, UP3). */
  logsAvailable: boolean;
  endpoint: string;
}

export interface ImportReportSent { id: string }
