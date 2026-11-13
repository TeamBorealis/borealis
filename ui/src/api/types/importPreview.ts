/** Rules-based content import with a preview (no AI). Mirrors `content_preview.rs`. */

import type { MigrationIssueCode } from "./content";
import type { BookRuleset } from "./core";
import type { HomebrewCharacterRef } from "./homebrew";

export interface ImportFileInput {
  fileName: string;
  /** File bytes, base64 encoded. */
  contentBase64: string;
}

/** Uploaded files and/or a folder token from `pickPath({ purpose: "content-folder" })` (desktop). */
export interface PreviewImportRequest {
  files?: ImportFileInput[];
  folder?: string;
}

export interface TypeCount {
  type: string;
  count: number;
}

export interface PreviewDiagnostic {
  severity: "info" | "warning" | "error";
  message: string;
  element: string | null;
  /** Path of the source file inside the imported set (e.g. "classes/wizard.xml"). */
  file: string | null;
  code: MigrationIssueCode;
  /** The element's type in the source ("Spell", "Class Feature", ...). */
  elementType: string | null;
  line: number | null;
}

/** An id that is neither in the imported set nor in the installed content. */
export interface UnresolvedRef {
  target: string;
  count: number;
  referencedBy: string[];
  /** Files of the set that reference it (a sample). */
  files: string[];
  /** The loaded book whose abbreviation the id carries: it defines no such id (misspelled, or a file of it is missing). */
  knownSource?: string;
}

export interface ImportPreview {
  previewId: string;
  packName: string;
  packId: string;
  fileCount: number;
  converted: number;
  skipped: number;
  counts: TypeCount[];
  summary: { errors: number; warnings: number; infos: number };
  diagnostics: PreviewDiagnostic[];
  diagnosticsTruncated: boolean;
  unresolved: UnresolvedRef[];
  unresolvedTotal: number;
  /** Unknown ids only requirements name and do without (an absence test, an alternative beside one that holds). */
  conditions: UnresolvedRef[];
  conditionsTotal: number;
  /** An installed pack with the same id will be replaced. */
  replacesExisting: boolean;
  /** A lone `.index` with nothing importable beside it: the address it updates from, offered as a content source. */
  indexUrl?: string;
  /** IM14: the books of the set and their editions; the user may change them before Install. */
  books: PreviewBook[];
  /** A homebrew pack file (`.borealis-pack.zip` with `creator.json`, spec 14.2): it installs editable in Homebrew. */
  homebrew?: HomebrewImportInfo;
}

/** What an import preview of a homebrew pack file adds. */
export interface HomebrewImportInfo {
  code: string;
  /** Another pack or book uses the code: only Import as a copy (or Replace, when this pack is installed). */
  codeTaken: boolean;
  /** The code and name Import as a copy gives. */
  copyCode: string;
  copyName: string;
  /** Replace (spec 10.3): the local characters that use an element of the installed pack this file lacks. */
  replaceUsedBy: HomebrewCharacterRef[];
  /** The names of those elements. */
  replaceDrops: string[];
}

/** IM14: a book of an import preview. */
export interface PreviewBook {
  name: string;
  /** What the importer found (IM17): the "D&D 5.5e" marker or the book saying 2024, `any` when it names both editions. */
  detected: BookRuleset;
  /** What will apply: the user's stored choice for this book, else the installed book's edition, else `detected`. */
  edition: BookRuleset;
  /** `edition` is a choice the user made earlier. */
  setByUser: boolean;
}

/** A file the OS handed to the app ("open with" / share). */
export interface OpenedFile {
  fileName: string;
  contentBase64: string;
}
