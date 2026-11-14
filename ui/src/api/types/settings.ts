import type { AbilityMethod } from "./character";
import type { Ruleset } from "./core";

export type ThemeMode = "system" | "light" | "dark";
export type Density = "compact" | "comfortable";

export type SheetTheme = "colour" | "ink";
export type SheetEditionOption = "auto" | "2014" | "2024";
/** Paper size of the sheet (TV1, B-147). */
export type SheetPaper = "letter" | "a4";

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
  /** Paper size; null until the user picks one, when the device region's paper applies (`sheetPaper`). */
  paper: SheetPaper | null;
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
  /** Read-only platform flag from the backend: true on store builds (the iOS app, the Google Play build), which never look for new
   * versions and hide the Releases link. Ignored in patches. */
  storeBuild: boolean;
  /** Read-only platform flag from the backend: this build has the AI CLI integration (false on iOS/Android and in the Mac App
   * Store build, where every AI entry is hidden and `ai.enabled` stays off). Ignored in patches. */
  aiAvailable: boolean;
  /** DATA FILES documents directory (characters in `<dir>/characters`); null = app default. Set only via `setDocumentsDir` (resend unchanged in `saveSettings`). */
  documentsDir: string | null;
  /** DATA FILES secondary content directory; null = unlinked. Set only via `setSecondaryContentDir` / `unlinkSecondaryContentDir`. */
  secondaryContentDir: string | null;
  /** CONTENT portrait library (GitHub #32): a folder of the user's own images shown in the portraits gallery; null = none. Set only via
   * `setPortraitLibraryDir` (resend unchanged in `saveSettings`). */
  portraitLibraryDir: string | null;
  /** CONTENT Obsidian vault (GitHub #15): the folder the notes view opens in place; null = none. Set only via `setVaultDir`
   * (resend unchanged in `saveSettings`). */
  vaultDir: string | null;
  /** Classic builder "Automatic Updates": check the user's indexes at startup (default on, desktop only). */
  autoContentUpdate: boolean;
  /** "Check for new versions of Borealis" (default on): at startup ask GitHub for the latest version number, nothing else. */
  checkAppUpdates: boolean;
  /** Sources page "Restrict playtest content" (default off): playtest books start unchecked for new characters. */
  restrictPlaytest: boolean;
  /** Sources page "Restrict third-party content" (default off): books not marked official start unchecked for new characters. */
  restrictThirdParty: boolean;
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
  /** The chosen documents directory while it cannot be used (unplugged drive, offline share): this session falls back to the default and the setting is kept. */
  documentsDirUnavailable: string | null;
  secondaryContentDir: string | null;
  /** The linked secondary directory cannot be read right now; the packs from it stay installed. */
  secondaryContentUnavailable: boolean;
  /** The chosen portrait library folder (Settings > CONTENT), if any. */
  portraitLibraryDir: string | null;
  /** The chosen portrait library folder cannot be read right now; the setting is kept. */
  portraitLibraryUnavailable: boolean;
  /** The chosen Obsidian vault folder (Settings > CONTENT, GitHub #15), if any. On Android a document tree URI. */
  vaultDir: string | null;
  /** Where the vault is, for people: the folder path, or on Android `Internal storage/Documents/My Vault`. */
  vaultLabel: string | null;
  /** The chosen vault folder cannot be read right now; the setting is kept. */
  vaultUnavailable: boolean;
}

/** One portrait library image (names only; read the image with `portraitLibraryImage`). */
export interface PortraitLibraryImage {
  /** `name`, or `folder/name` for an image one sub-folder down. */
  id: string;
  name: string;
  folder: string | null;
}

/** `listPortraitLibrary` result: the portraits gallery's library images. */
export interface PortraitLibrary {
  /** The chosen folder; null = no library. */
  dir: string | null;
  /** The chosen folder cannot be read right now (no images listed; the setting is kept). */
  unavailable: boolean;
  /** Name order: the folder's own images first, then each sub-folder's. */
  images: PortraitLibraryImage[];
  /** Image files that cannot be used: larger than 2 MiB, not really an image, or a link out of the folder. */
  refused: number;
  /** The folder holds more images than are listed. */
  truncated: boolean;
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
  /** The linked folder could not be read: nothing was installed or removed. */
  unavailable: boolean;
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
  /** How this copy gets the new version: "install" (Install and restart, plus Download; direct Windows, macOS and
   * AppImage installs once the updater key exists), "package-manager" (a deb or rpm install), "download". */
  updateMethod: UpdateMethod;
}

/** See `AppUpdateStatus.updateMethod`. */
export type UpdateMethod = "install" | "package-manager" | "download";

/** `appinstall:progress`: Install and restart is downloading the update. */
export interface AppInstallProgress {
  /** Bytes received so far. */
  downloaded: number;
  /** The size the server announced, when it did. */
  total: number | null;
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
  /** The device region's paper, used while `SheetSettings.paper` is null. */
  defaultPaper?: SheetPaper;
  settings?: Partial<SheetSettings>;
}

export interface SheetPreview {
  /** Self-contained HTML document for an iframe srcdoc preview. */
  html: string;
  pageCount: number;
  generatedAt: string;
}

/** `shareSheet` result. */
export interface SheetShare {
  /** File name of the shared PDF. */
  fileName: string;
  /** True when the backend opened the system share sheet (Android). */
  shared: boolean;
  /** Media type of the file. */
  mime: string;
  /** The bytes for the web view's own share sheet when the backend did not share (iOS, browser); null otherwise. */
  dataBase64: string | null;
}

export interface ExportResult {
  fileName: string;
  mime: string;
  /** Where the backend wrote the file: an absolute path (desktop) or the place the user picked (phones, B-148); else null. */
  path: string | null;
  /** Base64 bytes when there is no filesystem access (mobile share-sheet / browser). */
  dataBase64: string | null;
}
