/* ---------- Device transfer (Send to device / Receive from device, B3) ---------- */

/** Which user packs travel with the selected characters (default `required`). */
export type LinkPackChoice = "required" | "all" | "none";

/** What to send. */
export interface LinkSelection {
  /** Character ids (`c_...`); may be empty only with `packs: "all"`. */
  characterIds: string[];
  packs?: LinkPackChoice;
}

/** Stage of a Send/Receive session. */
export type LinkPhase =
  | "waiting"
  | "connecting"
  | "preview"
  | "waiting_for_peer"
  | "transferring"
  | "applying"
  | "done"
  | "failed"
  | "cancelled"
  | "declined"
  | "expired";

/** What Accept does with one item (newest edit wins; a replaced copy goes to the sync trash for 30 days). */
export type LinkPreviewAction = "new" | "replace" | "keep_local" | "identical";

export interface LinkPreviewItem {
  kind: "character" | "pack";
  key: string;
  /** Display name from the sending device (never a path). */
  label: string;
  action: LinkPreviewAction;
  size: number;
  /** User packs a character needs that are neither here nor in the transfer. */
  missingPacks: string[];
}

export interface LinkPreview {
  items: LinkPreviewItem[];
  totalBytes: number;
}

export interface LinkSummary {
  added: number;
  replaced: number;
  keptLocal: number;
  identical: number;
}

/** One session (also the `link:status` event payload). */
export interface LinkStatus {
  sessionId: string;
  /** What this device does. */
  role: "send" | "receive";
  phase: LinkPhase;
  /** Listening side: show as a QR code (`borealis-link:v1?...`). */
  qrPayload: string | null;
  /** Listening side: the typed code (`42-ember-lantern`), single use. */
  code: string | null;
  /** Listening side: `ip:port`, typed next to the code where discovery is unavailable (phones). */
  address: string | null;
  /** This device's short certificate fingerprint. */
  fingerprint: string | null;
  expiresInSecs: number | null;
  peerName: string | null;
  /** Receiver, phase `preview`: what Accept would do. */
  preview: LinkPreview | null;
  bytesDone: number | null;
  bytesTotal: number | null;
  /** Phase `done`. */
  summary: LinkSummary | null;
  /** Phase `failed`: `wrong_code`, `expired`, `refused`, `connection`, `pin_mismatch`, `not_found`, `protocol`, `local`, `peer_cancelled`. */
  errorCode: string | null;
  error: string | null;
}

/** Operating system of a paired device. */
export type LinkPlatform = "macos" | "windows" | "linux" | "ios" | "android" | "unknown";

/** One paired device (Settings > Devices). Times are unix seconds. */
export interface LinkDevice {
  id: string;
  /** The name it gave when pairing (display only). */
  name: string;
  platform: LinkPlatform;
  pairedAt: number;
  /** Last Send/Receive with it. */
  lastSyncedAt: number;
  /** The pairing lapses 180 days after the last transfer; re-pair by QR or code. */
  expiresAt: number;
}

/* ---------- Bundles as files (Export to bundle / Open bundle / Share as a file, A2) ---------- */

/** A `.borealis` file opened for preview: the same rows a device transfer shows. Nothing is written until `bundleAccept`. */
export interface BundlePreview {
  /** Pass to `bundleAccept` / `bundleDiscard`. */
  bundleId: string;
  /** The file's name (never a path). */
  fileName: string;
  /** The device that made the bundle (display only). */
  sourceDevice: string;
  preview: LinkPreview;
}

/** `bundleExport` result (desktop). `path` is null when the user cancelled the save dialog. */
export interface BundleExport {
  fileName: string;
  path: string | null;
  characters: number;
  packs: number;
  bytes: number;
}

/** `bundleShare` result (phones). Android opens the system share sheet itself (`shared`); elsewhere `dataBase64` is for the web view's share sheet. */
export interface BundleShare {
  fileName: string;
  characters: number;
  packs: number;
  bytes: number;
  shared: boolean;
  dataBase64: string | null;
}
