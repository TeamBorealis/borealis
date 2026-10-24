/**
 * Backend-owned file picking (security: the webview never sends filesystem paths).
 * `pickPath` opens the native dialog in Rust and returns an opaque, single-purpose token that
 * expires after 10 minutes; commands that read files accept only that token.
 */
export type PickPurpose = "import-file" | "5etools-folder" | "content-folder" | "ai-override" | "documents-dir" | "secondary-content-dir"
  | "character-file"
  /** A `.borealis` bundle for `bundleOpen` (opened by path in the backend, never read into the webview). */
  | "bundle-file"
  /** Minted only by the backend's native drag-drop handler; `pickPath` refuses it. */
  | "dropped-file";

export interface PickedPath {
  /** Pass to the command that uses the pick (`importStart` file source, 5etools, `setAiOverride`, `setDocumentsDir`, `setSecondaryContentDir`). */
  token: string;
  /** File or folder name. */
  name: string;
  /** Full path, for display only. */
  path: string;
}

/** One file of a native drop: read it with `takePickedFiles({ purpose: "dropped-file" })`. */
export interface DroppedFile {
  token: string;
  name: string;
}

/**
 * `files:dropped` payload. `rejected` names dropped items that cannot be imported (other file types, a second folder).
 * `folders` holds at most one dropped folder as a `content-folder` token, previewed like the folder picker's.
 */
export interface FileDrop {
  files: DroppedFile[];
  folders: DroppedFile[];
  /** `.borealis` bundles as `bundle-file` tokens: open them with `bundleOpen`. */
  bundles: DroppedFile[];
  rejected: string[];
}
