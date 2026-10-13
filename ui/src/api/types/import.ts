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
  maxPdfBytes: 20 * 1024 * 1024,
  maxTextChars: 200_000,
  /** Typed notes that accompany images or PDFs in one `import_start`. */
  maxNotesChars: 4000,
} as const;

export interface ImportHint {
  kind?: ImportKind;
  edition?: Ruleset;
  parentClassId?: string;
}

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
}

/**
 * Text sources may be mixed with images/PDFs: when files are present the text is not the source but
 * the user's notes ("this is a 2024 Fighter subclass"), sent to the model next to the files.
 */
/** `allowUntrustedWithCodex`: the user agreed to run this import on Codex (asked once per import after `consent_required`). */
export interface ImportStartRequest { sources: ImportSource[]; hint?: ImportHint; allowUntrustedWithCodex?: boolean }
export interface ImportFinalizeRequest { packName: string; prefix: string }
