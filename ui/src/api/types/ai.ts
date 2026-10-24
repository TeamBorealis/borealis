export type AiMode =
  | "chat"
  | "homebrew"
  | "rules_qa"
  | "build_advisor"
  | "backstory"
  | "explain"
  | "npc";

export type AiAvailability = "ready" | "unavailable" | "needs_pairing";

export interface AiProviderInfo {
  backend: "claude" | "codex";
  path: string;
  version: string | null;
  source: "override" | "path" | "known_location";
}

export interface AiStatus {
  availability: AiAvailability;
  provider: "claude" | "codex" | "none";
  /** Provider CLI version, e.g. "claude 2.1". */
  version: string | null;
  /** "local" CLI, a paired "desktop" host (mobile), or none. */
  host: "local" | "desktop" | null;
  detail: string;
  /** Every provider found on this machine, preferred first (empty on mobile). */
  providers?: AiProviderInfo[];
}

export interface AiMessage {
  role: "user" | "assistant";
  text: string;
}

export interface AiTaskRequest {
  mode: AiMode;
  prompt: string;
  characterId?: string | null;
  history?: AiMessage[];
  /** Mode parameters: explain `{ stat }`; backstory/npc `{ tone?, length?, kind? }`. */
  params?: Record<string, unknown>;
}

export interface AiTaskHandle {
  taskId: string;
}

export interface HomebrewDraft {
  kind: "homebrew";
  /** Element JSON in the content schema, not yet validated by the UI. */
  json: Record<string, unknown>;
}

/** Typed result of a non-homebrew task. `data` keys are camelCase; see docs/ui-api-contract.md for the shapes. */
export type AiTypedResult =
  | { kind: "rules_qa"; data: CitedAnswerData }
  | { kind: "explain" | "level_up" | "narrative"; data: Record<string, unknown> };

/** One validated citation of a `rules_qa` answer, resolved to its content element. */
export interface CitedSource {
  id: string;
  name: string;
  type: string;
}

/** `rules_qa` data: only ids that were retrieved survive in `citations`. */
export interface CitedAnswerData {
  /** Answer text with inline `[[ELEMENT_ID]]` citation markers (unknown ids already stripped). */
  answerMarkdown: string;
  citations: string[];
  sources: CitedSource[];
  confidence: "high" | "medium" | "low";
  notCovered: boolean;
  strippedCitations: string[];
  uncitedClaims: string[];
  unverified: boolean;
}

export type AiEvent =
  | { taskId: string; kind: "started"; mode: AiMode }
  | { taskId: string; kind: "delta"; text: string }
  | { taskId: string; kind: "tool"; name: string; summary: string }
  | { taskId: string; kind: "result"; result: HomebrewDraft | AiTypedResult }
  | { taskId: string; kind: "error"; message: string }
  | { taskId: string; kind: "done"; cancelled: boolean };

export interface ValidationReport {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

export interface AiHostDevice {
  id: string;
  name: string;
}

/** Desktop LAN AI host (phones pair with it). `supported` is false on mobile builds. */
export interface AiHostStatus {
  enabled: boolean;
  supported: boolean;
  /** `ip:port` the phone connects to. */
  address: string | null;
  pairingCode: string | null;
  pairingExpiresInSecs: number | null;
  /** `borealis-pair:` string for the QR code; only while a pairing window is open. */
  pairingUri: string | null;
  /** Short certificate fingerprint to compare on the phone; while the host runs. */
  fingerprint: string | null;
  devices: AiHostDevice[];
}

/* ---------- First-run AI setup ---------- */

export type AiSetupProviderId = "claude" | "codex";

export interface AiSetupProvider {
  backend: AiSetupProviderId;
  installed: boolean;
  path: string | null;
  version: string | null;
  /** `null` when not installed or the check was inconclusive. */
  loggedIn: boolean | null;
  /** What installing runs, e.g. `npm install -g @anthropic-ai/claude-code`. */
  installCommand: string;
  /** What the user runs to sign in. */
  loginCommand: string;
}

export interface AiSetupStatus {
  providers: AiSetupProvider[];
  /** Path of `npm` when found; without it the screen shows manual instructions. */
  npmPath: string | null;
  profile: { state: "installed" | "updated" | "verified" | "repaired" | "unavailable" | "error"; detail: string[] };
  /** The user finished or skipped the setup before. */
  completed: boolean;
  /** Phones pair with a desktop instead. */
  mobile: boolean;
}

/** One-time consent from the confirm step; required by `aiSetupInstall`. */
export interface AiInstallConsent {
  token: string;
  /** The exact command that will run. */
  command: string;
}

export interface AiLoginHelp {
  command: string;
  /** A terminal window was opened with the command (macOS). */
  opened: boolean;
}

export type AiSetupEvent =
  | { kind: "output"; provider: AiSetupProviderId; line: string }
  | { kind: "done"; provider: AiSetupProviderId; ok: boolean; message: string };
