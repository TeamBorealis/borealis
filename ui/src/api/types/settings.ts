import type { AbilityMethod } from "./character";
import type { Ruleset } from "./core";

export type ThemeMode = "system" | "light" | "dark";
export type Density = "compact" | "comfortable";

export type SheetTheme = "colour" | "ink";
export type SheetEditionOption = "auto" | "2014" | "2024";

export interface SheetSettings {
  /** Fillable PDF form fields (default false: the PDF is flattened); on keeps the fields editable. */
  formFillable: boolean;
  /** Export colour treatment (default "colour"); "ink" is monochrome. */
  theme: SheetTheme;
  /** Sheet edition override (default "auto" = the character's ruleset). */
  edition: SheetEditionOption;
  styleFormatting: boolean;
  swapAbilityPositions: boolean;
  includeNonPreparedSpells: boolean;
  cards: { spell: boolean; item: boolean; attack: boolean; feature: boolean };
  autoGenerate: boolean;
  openAfterSave: boolean;
  /** Classic builder "Start ... Cards on a New Page" sub-options (defaults item on, attack off, feature on). */
  cardsNewPage: { item: boolean; attack: boolean; feature: boolean };
  /** Classic builder "Legacy Spellcasting Page" (default off). */
  legacySpellcastingPage: boolean;
}

/** Classic builder selection-picker ROW HEIGHT (default "medium"). */
export type RowHeight = "small" | "medium" | "large";
/** Classic builder accent swatches in dialog order; "default" = "Default". */
export type AccentColor = "black" | "brown" | "default" | "purple" | "green" | "aqua" | "blue" | "mauve" | "pink" | "red";

export type AiProviderId = "claude" | "codex" | "none";

export interface AiSettings {
  /** Optional AI assistant: off by default; always false on mobile. */
  enabled: boolean;
  provider: AiProviderId;
  /** Mobile: host address of a paired desktop instance. */
  pairedHost: string | null;
  /** Explicit path to the preferred provider's executable (desktop). */
  overridePath: string | null;
}

/** Local search settings (local-search-spec.md section 9). */
export interface SearchSettings {
  /** Semantic search (consented model download, not built yet; default off). */
  semantic: boolean;
  tierOverride: "auto" | "t1" | "t2" | "t3";
  /** Mobile: build semantic indexes on battery too (default off). */
  indexOnBattery: boolean;
  /** Edition the search flyout opens with ("character" = the open character's edition, both without one). */
  defaultEdition: "character" | "both" | "2014" | "2024";
}

export interface AppSettings {
  playerName: string;
  defaultRuleset: Ruleset;
  abilityMethod: AbilityMethod;
  pointBuyBudget: number;
  abilityMinBase: number;
  abilityMaxBase: number;
  abilityCapEnabled: boolean;
  selectionFlow: boolean;
  confirmRemoveLevel: boolean;
  confirmEquipWeaponNotInAttacks: boolean;
  density: Density;
  theme: ThemeMode;
  highContrast: boolean;
  tileSize: number;
  /** Autosave draft (default ON). */
  autosave: boolean;
  sheet: SheetSettings;
  ai: AiSettings;
  /** Read-only platform flag from the backend: true on iOS/Android builds (no AI there). Ignored in patches. */
  mobile: boolean;
  /** DATA FILES documents directory (characters in `<dir>/characters`); null = app default. Set only via `setDocumentsDir` (resend unchanged in `saveSettings`). */
  documentsDir: string | null;
  /** DATA FILES secondary content directory; null = unlinked. Set only via `setSecondaryContentDir` / `unlinkSecondaryContentDir`. */
  secondaryContentDir: string | null;
  /** Classic builder "Automatic Updates": check the user's indexes at startup (default on, desktop only). */
  autoContentUpdate: boolean;
  /** "Check for new versions of Borealis" (default on): at startup ask GitHub for the latest version number, nothing else. */
  checkAppUpdates: boolean;
  rowHeight: RowHeight;
  /** Classic builder "Skills Panel Expanded" (default on). */
  skillsPanelExpanded: boolean;
  /** Classic builder "Saving Throws Expanded" (default off). */
  savingThrowsExpanded: boolean;
  accentColor: AccentColor;
  /** Classic builder CHARACTER COLLECTION PORTRAIT SIZE: 125, 150 (default) or 175 px. */
  portraitSize: number;
  /** Show the "Import only content you own" notice before every content import (default on; "Don't show this again" turns it off). */
  showImportNotice: boolean;
  search: SearchSettings;
}

/** Settings > DATA FILES folders (display only; paths never go back to the backend). */
export interface DataDirs {
  documentsDir: string;
  defaultDocumentsDir: string;
  isDefault: boolean;
  /** Where character files are read and saved (`<documentsDir>/characters`). */
  charactersDir: string;
  secondaryContentDir: string | null;
}

export interface SkippedContentFile {
  /** Path relative to the secondary directory. */
  file: string;
  reason: string;
}

export interface SecondaryContentReport {
  /** Pack ids (`sec-*`) installed or refreshed. */
  installed: string[];
  removed: string[];
  skipped: SkippedContentFile[];
}

export interface SecondaryContentResult {
  dirs: DataDirs;
  report: SecondaryContentReport;
}

/** `clearCaches` result (classic builder "Clear News Articles Cache"; Borealis has no news feed). */
export interface ClearCachesResult {
  tempFoldersRemoved: number;
  aiTempFoldersRemoved: number;
  indexesReset: number;
}

/** `idle` = not asked yet, `disabled` = setting off, `unsupported` = the UI asks GitHub itself (phones), `failed` = offline or rate limited. */
export type AppUpdateState = "idle" | "disabled" | "done" | "failed" | "unsupported";

/** Whether a newer Borealis release exists (`appUpdateStatus`, `checkAppUpdate`, event `appupdate:status`). */
export interface AppUpdateStatus {
  state: AppUpdateState;
  /** This build's version. */
  current: string;
  /** Latest release version without a leading "v". */
  latest: string | null;
  newer: boolean;
  /** Where to download: the website download page. */
  url: string | null;
  /** The GitHub release page, when GitHub gave one. */
  releaseUrl: string | null;
  /** The release notes (GitHub's markdown), trimmed and capped; null when there are none. */
  notes: string | null;
  /** RFC 3339. */
  checkedAt: string | null;
}

export type ContentUpdateState = "idle" | "disabled" | "pending" | "running" | "done" | "failed";

/** App-owned folders the backend can open (the UI never passes a path). */
export type AppFolder = "content" | "user-content" | "characters" | "logs";

/** Outcome of the automatic content update at startup (also sent on `content:progress`). */
export interface ContentUpdateStatus {
  state: ContentUpdateState;
  /** Status-bar text. */
  message: string;
  updated: number;
  failed: string[];
  /** RFC 3339; render classic builder's "Last checked for content updates at <time>" from it. */
  checkedAt: string | null;
}

export interface SheetOptions {
  format?: "pdf" | "html";
  settings?: Partial<SheetSettings>;
}

export interface SheetPreview {
  /** Self-contained HTML document for an iframe srcdoc preview. */
  html: string;
  pageCount: number;
  generatedAt: string;
}

export interface ExportResult {
  fileName: string;
  mime: string;
  /** Absolute path when the backend wrote the file (desktop); else null. */
  path: string | null;
  /** Base64 bytes when there is no filesystem access (mobile share-sheet / browser). */
  dataBase64: string | null;
}
