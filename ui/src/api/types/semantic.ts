/* Semantic search model (local-search-spec.md section 8, S3.2): consented download, pause / cancel, delete. */

/** Device tier: T0 has no semantic option; T1 phones 3-4 GB; T2 bigger phones and small desktops; T3 desktops >= 8 GB. */
export type SemanticTier = "t0" | "t1" | "t2" | "t3";

export type SemanticDownloadState = "idle" | "downloading" | "verifying" | "paused" | "failed" | "done";

/** `semantic:download` payload (also `SemanticStatus.download`). */
export interface SemanticDownload {
  state: SemanticDownloadState;
  /** Bytes downloaded (or verified while verifying). */
  done: number;
  total: number;
  /** Plain sentence: failure reason, background pause, or "semantic search arrives in a later update". */
  message: string | null;
}

/** The model file this device downloads. */
export interface SemanticFile {
  quant: string;
  fileName: string;
  bytes: number;
  /** Consent size text ("about 310 MB", "210 MB"). */
  sizeText: string;
}

/** Consent dialog text, spec wording with this device's sizes. */
export interface SemanticConsent {
  title: string;
  body: string;
  accept: string;
  decline: string;
}

/** `semanticStatus` result (Settings > Search). Never causes a request. */
export interface SemanticStatus {
  /** Tier in effect (probe lowered by `settings.search.tierOverride`). */
  tier: SemanticTier;
  /** Highest tier the device allows; the override can only go below it. */
  probedTier: SemanticTier;
  mobile: boolean;
  file: SemanticFile | null;
  /** A URL serves the file (false for the T1 file until the mirror exists). */
  available: boolean;
  unavailableReason: string | null;
  modelPresent: boolean;
  /** An embedding backend is linked into this build (false until B-46). */
  backendAvailable: boolean;
  /** `settings.search.semantic`. */
  semantic: boolean;
  /** Search really uses embeddings: semantic on, model present, backend linked. */
  active: boolean;
  consent: SemanticConsent | null;
  download: SemanticDownload;
}

/** `semanticModelLicense` result: shown in Settings > About. */
export interface SemanticLicense {
  model: string;
  license: string;
  notice: string;
}
