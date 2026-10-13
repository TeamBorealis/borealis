/** Rules-based content import with a preview (no AI). Mirrors `content_preview.rs`. */

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
  /** File name only. */
  file: string | null;
}

/** An id that is neither in the imported set nor in the installed content. */
export interface UnresolvedRef {
  target: string;
  count: number;
  referencedBy: string[];
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
  /** An installed pack with the same id will be replaced. */
  replacesExisting: boolean;
}

/** A file the OS handed to the app ("open with" / share). */
export interface OpenedFile {
  fileName: string;
  contentBase64: string;
}
