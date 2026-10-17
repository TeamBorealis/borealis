/**
 * Backend contract between the Borealis UI and the Rust/Tauri side.
 *
 * Design rules:
 *  - Coarse-grained: every character mutation returns the full new `CharacterSnapshot`.
 *  - Each method maps to exactly one Tauri command (snake_case, see `COMMANDS`).
 *    The method's argument object is the invoke payload: its top-level keys are the command's
 *    named parameters (Tauri maps camelCase keys to snake_case Rust parameters).
 *  - Failures reject with an `ApiError`.
 *  - Streaming / push data uses `subscribe` (Tauri events, see `EVENTS`).
 */
import type {
  AbilityKey,
  AbilityMethod,
  AiEvent,
  AiHostStatus,
  AiInstallConsent,
  AiLoginHelp,
  AiSetupEvent,
  AiSetupProviderId,
  AiSetupStatus,
  EncounterEvaluateArgs,
  EncounterSuggestArgs,
  AiMessage,
  AiMode,
  AiStatus,
  AiTaskHandle,
  AiTaskRequest,
  AppSettings,
  AppUpdateStatus,
  AppFolder,
  ClearCachesResult,
  ContentUpdateStatus,
  DataDirs,
  SecondaryContentResult,
  FileDrop,
  ImportFileInput,
  PickPurpose,
  PickedPath,
  CharacterDetails,
  CharacterOptions,
  CharacterSnapshot,
  CharacterSummary,
  CoinKey,
  CompanionState,
  CompendiumFacets,
  CompendiumQuery,
  ContentIndex,
  ContentPack,
  ContentProgress,
  FiveToolsImportReport,
  FiveToolsUrl,
  FiveToolsUrlUpdate,
  ImportDraft,
  ImportEvent,
  ImportFinalizeRequest,
  ImportReportPreview,
  ImportReportRequest,
  ImportReportSent,
  ImportStartRequest,
  FiveToolsImportRequest,
  FiveToolsScan,
  ContentSource,
  ElementDetail,
  ElementSummary,
  EquipRequest,
  ExportResult,
  ImportPackRequest,
  ImportPackResult,
  ImportPreview,
  OpenedFile,
  PreviewImportRequest,
  ImportReport,
  RecheckReport,
  InventoryItem,
  ItemQuery,
  ItemSummary,
  NewCharacterRequest,
  Page,
  RestrictionPreview,
  CompatibilityReport,
  RulesetProfile,
  SheetOptions,
  SheetPreview,
  StatTrace,
  Ruleset,
  Unsubscribe,
  UpdateIndexesResult,
  ValidationReport,
  Attack,
  ConversionApplied,
  ConversionPreview,
} from "./types";

export * from "./types";

type Cid = { characterId: string };

export interface SlotOptionsArgs extends Cid {
  slotId: string;
  query?: string;
  limit?: number;
  offset?: number;
}
export interface SlotOption extends ElementSummary {
  /** Present when the option is shown but not choosable (e.g. unmet prerequisite). */
  disabledReason?: string;
}

/** All request/response commands. Keep in sync with `COMMANDS`. */
export interface ApiMethods {
  /* App */
  appVersion(): Promise<string>;

  /* Characters (library) */
  listCharacters(): Promise<CharacterSummary[]>;
  createCharacter(args: { request: NewCharacterRequest }): Promise<CharacterSnapshot>;
  loadCharacter(args: Cid): Promise<CharacterSnapshot>;
  getSnapshot(args: Cid): Promise<CharacterSnapshot>;
  /** Silent save to the existing path (autosave uses the same command). */
  saveCharacter(args: Cid): Promise<CharacterSnapshot>;
  /** Save As: with `name` a new library copy; without, the backend's native save dialog picks a `.json` file (desktop). */
  saveCharacterAs(args: Cid & { name?: string }): Promise<CharacterSnapshot>;
  closeCharacter(args: Cid & { discard: boolean }): Promise<void>;
  deleteCharacter(args: Cid): Promise<void>;
  duplicateCharacter(args: Cid): Promise<CharacterSummary>;
  updateCharacterMeta(
    args: Cid & { name?: string; group?: string; favorite?: boolean },
  ): Promise<CharacterSnapshot>;
  setPortrait(args: Cid & { dataUrl: string | null }): Promise<CharacterSnapshot>;
  /** Import a legacy `.dnd5e` (or native JSON) file from bytes. */
  importCharacter(args: { fileName: string; contentBase64: string }): Promise<ImportReport>;
  /** Retry imported characters' missing content against what is installed now (also runs after every content install). */
  recheckMissingContent(): Promise<RecheckReport>;
  exportCharacter(args: Cid): Promise<ExportResult>;

  /* Builder: selections */
  getSlotOptions(args: SlotOptionsArgs): Promise<Page<SlotOption>>;
  applySelection(args: Cid & { slotId: string; elementId: string }): Promise<CharacterSnapshot>;
  clearSelection(args: Cid & { slotId: string }): Promise<CharacterSnapshot>;
  replaceSpell(args: Cid & { slotId: string }): Promise<CharacterSnapshot>;

  /* Builder: levels */
  setXp(args: Cid & { xp: number }): Promise<CharacterSnapshot>;
  levelUp(args: Cid & { classSlotId: string }): Promise<CharacterSnapshot>;
  levelDown(args: Cid): Promise<CharacterSnapshot>;
  addMulticlass(args: Cid): Promise<CharacterSnapshot>;
  setHpRoll(args: Cid & { classSlotId: string; level: number; value: number }): Promise<CharacterSnapshot>;

  /* Builder: abilities (backend enforces budgets) */
  setAbilityMethod(args: Cid & { method: AbilityMethod }): Promise<CharacterSnapshot>;
  setBaseScore(args: Cid & { ability: AbilityKey; value: number }): Promise<CharacterSnapshot>;
  rollAbilities(args: Cid & { ability?: AbilityKey }): Promise<CharacterSnapshot>;
  /** Assign (or swap) a standard-array value; `value: null` unassigns. */
  assignStandardArray(
    args: Cid & { ability: AbilityKey; value: number | null },
  ): Promise<CharacterSnapshot>;
  resetAbilities(args: Cid): Promise<CharacterSnapshot>;

  /* Builder: options, restrictions, ruleset */
  setCharacterOptions(args: Cid & { options: Partial<CharacterOptions> }): Promise<CharacterSnapshot>;
  previewSourceRestrictions(args: Cid & { enabledSources: string[] | null }): Promise<RestrictionPreview>;
  setSourceRestrictions(args: Cid & { enabledSources: string[] | null }): Promise<CharacterSnapshot>;
  /** `allow: null` resets to the ruleset default (strict for 2014, open for 2024). Stored picks are never dropped. */
  setAllowCrossEdition(args: Cid & { allow: boolean | null }): Promise<CharacterSnapshot>;
  getCompatibilityReport(args: Cid): Promise<CompatibilityReport>;
  getRulesetProfile(args: { ruleset: Ruleset }): Promise<RulesetProfile>;

  /* Magic */
  setSpellPrepared(
    args: Cid & { classSlotId: string; spellId: string; prepared: boolean },
  ): Promise<CharacterSnapshot>;

  /* 2024: play state and Convert to 2024. All return the full snapshot (`play` block). */
  /** What converting a 2014 character would keep, map, drop or ask for. Saves nothing. */
  previewConvertTo2024(args: Cid): Promise<ConversionPreview>;
  /** Saves the converted copy as a new character and opens it; the original is untouched. */
  applyConvertTo2024(args: Cid): Promise<ConversionApplied>;
  spendResource(args: Cid & { resourceId: string; uses?: number }): Promise<CharacterSnapshot>;
  restoreResource(args: Cid & { resourceId: string; uses?: number }): Promise<CharacterSnapshot>;
  shortRest(args: Cid): Promise<CharacterSnapshot>;
  /** Refills resources, ends toggles, exhaustion -1 (2024). */
  longRest(args: Cid): Promise<CharacterSnapshot>;
  setToggle(args: Cid & { toggleId: string; on: boolean }): Promise<CharacterSnapshot>;
  setExhaustion(args: Cid & { level: number }): Promise<CharacterSnapshot>;
  setHeroicInspiration(args: Cid & { has: boolean }): Promise<CharacterSnapshot>;
  /** Replaces all Weapon Mastery picks (in slot order; fewer ids leave slots open). */
  chooseWeaponMasteries(args: Cid & { weaponIds: string[] }): Promise<CharacterSnapshot>;
  setWildShapeForms(args: Cid & { formIds: string[] }): Promise<CharacterSnapshot>;
  /** Takes starting-equipment option `option` ("A", "B", ...) of a package from `play.equipmentPackages`. */
  chooseEquipmentPackage(args: Cid & { packageId: string; option: string }): Promise<CharacterSnapshot>;

  /* Equipment */
  searchItems(args: ItemQuery & { characterId?: string }): Promise<Page<ItemSummary>>;
  addItem(args: Cid & { itemId: string; quantity: number; buy: boolean; baseItemId?: string }): Promise<CharacterSnapshot>;
  removeItem(args: Cid & { inventoryId: string; all: boolean }): Promise<CharacterSnapshot>;
  equipItem(args: Cid & { inventoryId: string; slot: EquipRequest }): Promise<CharacterSnapshot>;
  unequipItem(args: Cid & { inventoryId: string }): Promise<CharacterSnapshot>;
  setAttuned(args: Cid & { inventoryId: string; attuned: boolean }): Promise<CharacterSnapshot>;
  /** Enter the maximum of a dice-valued charge resource (`InventoryItem.rolledCharges`); refused outside min..max. */
  setRolledMax(args: Cid & { inventoryId: string; resourceId: string; value: number }): Promise<CharacterSnapshot>;
  /** Roll that maximum again. */
  rerollMax(args: Cid & { inventoryId: string; resourceId: string }): Promise<CharacterSnapshot>;
  storeItem(args: Cid & { inventoryId: string; storage: 0 | 1 | 2 }): Promise<CharacterSnapshot>;
  moveItem(args: Cid & { inventoryId: string; direction: "up" | "down" }): Promise<CharacterSnapshot>;
  updateItemDetails(
    args: Cid & {
      inventoryId: string;
      patch: Partial<Pick<InventoryItem, "displayName" | "notes" | "listInInventory" | "showCard">>;
    },
  ): Promise<CharacterSnapshot>;
  extractPack(args: Cid & { inventoryId: string }): Promise<CharacterSnapshot>;
  adjustCoins(args: Cid & { coin: CoinKey; delta: number }): Promise<CharacterSnapshot>;
  convertCoins(args: Cid & { to: CoinKey }): Promise<CharacterSnapshot>;
  setStorageNames(args: Cid & { names: [string, string] }): Promise<CharacterSnapshot>;

  /* Explain: the deterministic provenance trace of one statistic */
  explainStat(args: Cid & { stat: string }): Promise<StatTrace>;

  /* Manage */
  updateDetails(args: Cid & { patch: Partial<CharacterDetails> }): Promise<CharacterSnapshot>;
  upsertAttack(args: Cid & { attack: Omit<Attack, "id"> & { id?: string; asCard?: boolean } }): Promise<CharacterSnapshot>;
  removeAttack(args: Cid & { attackId: string }): Promise<CharacterSnapshot>;
  moveAttack(args: Cid & { attackId: string; direction: "up" | "down" }): Promise<CharacterSnapshot>;
  updateCompanion(args: Cid & { patch: Partial<Pick<CompanionState, "name" | "portrait">> }): Promise<CharacterSnapshot>;

  /* Sheet */
  renderSheet(args: Cid & { options?: SheetOptions }): Promise<SheetPreview>;
  exportSheet(args: Cid & { options?: SheetOptions }): Promise<ExportResult>;

  /* Compendium */
  searchCompendium(args: { query: CompendiumQuery }): Promise<Page<ElementSummary>>;
  getElement(args: { elementId: string }): Promise<ElementDetail>;
  compendiumFacets(): Promise<CompendiumFacets>;

  /* Content */
  listPacks(): Promise<ContentPack[]>;
  importPack(args: { request: ImportPackRequest }): Promise<ImportPackResult>;
  /** Convert files or a picked folder without installing; `installImportPreview` then installs exactly that result. */
  previewImportPack(args: { request: PreviewImportRequest }): Promise<ImportPreview>;
  installImportPreview(args: { previewId: string }): Promise<ImportPackResult>;
  cancelImportPreview(args: { previewId: string }): Promise<void>;
  /** Files the OS opened with the app (macOS/iOS/Android "open with" / share), oldest first. */
  takeOpenedFiles(): Promise<OpenedFile[]>;
  setPackEnabled(args: { packId: string; enabled: boolean }): Promise<ContentPack[]>;
  removePack(args: { packId: string }): Promise<ContentPack[]>;
  listSources(): Promise<ContentSource[]>;
  listIndexes(): Promise<ContentIndex[]>;
  addIndex(args: { url: string }): Promise<ContentIndex[]>;
  removeIndex(args: { url: string }): Promise<ContentIndex[]>;
  updateIndexes(): Promise<UpdateIndexesResult>;
  clearDownloadedContent(): Promise<ContentPack[]>;
  /** Read-only scan of the user's own 5etools `data/` folder; lists the source books found. */
  scan5etoolsFolder(args: { folder: string }): Promise<FiveToolsScan>;
  /** Unpack a `.zip` of the user's 5etools `data/` folder (phones have no folder picker; desktop accepts it too). Returns a `5etools-folder` token for `scan5etoolsFolder` and `import5etools`. */
  stage5etoolsZip(args: { file: ImportFileInput }): Promise<PickedPath>;
  /** Download the 5etools `data/` folder at an address the user pasted (a GitHub repository or any https:// folder address; none ships with the app). Emits `import5etools:progress` while downloading. Returns a `5etools-folder` token for `scan5etoolsFolder` and `import5etools`; importing from it remembers the address. */
  stage5etoolsUrl(args: { url: string }): Promise<PickedPath>;
  /** Addresses 5etools data was imported from. */
  list5etoolsUrls(): Promise<FiveToolsUrl[]>;
  /** Forget an address; its packs stay installed. */
  remove5etoolsUrl(args: { url: string }): Promise<FiveToolsUrl[]>;
  /** Check an address and re-import the same books only when it changed. */
  update5etoolsUrl(args: { url: string }): Promise<FiveToolsUrlUpdate>;
  /** Convert the chosen books and install the packs; emits `content:progress`. */
  import5etools(args: { request: FiveToolsImportRequest }): Promise<FiveToolsImportReport>;

  /* Settings */
  getSettings(): Promise<AppSettings>;
  /** `ai.overridePath` can only be resent unchanged or cleared (null) here; see `setAiOverride`. */
  saveSettings(args: { patch: Partial<AppSettings> }): Promise<AppSettings>;

  /* Native pickers, opened by the backend; `[]` when cancelled. `startDir` only positions the dialog. */
  pickPath(args: { purpose: PickPurpose; startDir?: string }): Promise<PickedPath[]>;
  /** Read `character-file` picks or `dropped-file` tokens (from `files:dropped`); each token works once. */
  takePickedFiles(args: { purpose: PickPurpose; tokens: string[] }): Promise<OpenedFile[]>;
  /** Use an `ai-override` pick as the AI program after validation and a native confirmation; a declined confirmation returns the settings unchanged. */
  setAiOverride(args: { token: string }): Promise<AppSettings>;

  /* Settings > DATA FILES and the classic builder's deferred apply (Save = `saveSettings` with the full form, Cancel = `getSettings`). */
  /** Current documents / characters / secondary folders (display only). */
  getDataDirs(): Promise<DataDirs>;
  /** Use a `documents-dir` pick; omit `token` to go back to the default folder. Existing characters are not moved. */
  setDocumentsDir(args: { token?: string }): Promise<DataDirs>;
  /** Link a `secondary-content-dir` pick and load its .xml/.json files as `sec-*` user packs (hot-reload). */
  setSecondaryContentDir(args: { token: string }): Promise<SecondaryContentResult>;
  /** Classic builder "UNLINK SECONDARY DIRECTORY": forget the folder and remove its packs (hot-reload). */
  unlinkSecondaryContentDir(): Promise<SecondaryContentResult>;
  /** Classic builder "Restore Defaults": reset and save at once (AI, default ruleset and autosave are kept). */
  restoreDefaultSettings(): Promise<AppSettings>;
  /** Classic builder "Clear News Articles Cache" mapped to Borealis's caches (scratch folders, AI temp folders, index check results). Immediate, no Save needed. */
  clearCaches(): Promise<ClearCachesResult>;
  /** Outcome of the automatic content update at startup. */
  contentUpdateStatus(): Promise<ContentUpdateStatus>;
  /** Outcome of the startup "new version available" check (`unsupported` on phones, where the UI asks GitHub itself). */
  appUpdateStatus(): Promise<AppUpdateStatus>;
  /** "Check now": ask GitHub for the latest Borealis version right away (desktop; `unsupported` on phones). */
  checkAppUpdate(): Promise<AppUpdateStatus>;
  /** Open the official download page in the default browser (desktop; phones fail and the UI opens it). */
  openDownloadPage(): Promise<void>;
  /** Open an app-owned folder in the OS file manager. The backend resolves the path; the UI never sends one. */
  openAppFolder(args: { folder: AppFolder }): Promise<void>;
  /** The newest log lines (home folder shown as `~`), for sharing from a phone, which has no folder to open. */
  getRecentLogs(): Promise<string>;
  /** Restart the application so new content loads. */
  restartApp(): Promise<void>;

  /* AI */
  aiStatus(): Promise<AiStatus>;
  aiStartTask(args: { request: AiTaskRequest }): Promise<AiTaskHandle>;
  aiCancelTask(args: { taskId: string }): Promise<void>;
  aiValidateHomebrew(args: { json: Record<string, unknown> }): Promise<ValidationReport>;
  aiAddHomebrewToPack(args: { json: Record<string, unknown>; packId?: string }): Promise<ContentPack>;
  /** Apply the first levels of a build plan (`plan` as returned in `build_plan` data) to an open character. */
  applyPlan(args: Cid & { plan: Record<string, unknown>; upToLevel: number }): Promise<CharacterSnapshot>;
  /** Deterministic encounter verdict (no model): `{ report, monsters }`. */
  encounterEvaluate(args: { args: EncounterEvaluateArgs }): Promise<Record<string, unknown>>;
  /** Deterministic encounter for a band (no model). */
  encounterSuggest(args: { args: EncounterSuggestArgs }): Promise<Record<string, unknown>>;
  /** Desktop LAN host (see `AiHostStatus`). */
  /** Throws `consent_required` when only Codex is installed; retry with `allowUntrustedWithCodex: true` after the user agrees. */
  aiHostEnable(args?: { allowUntrustedWithCodex?: boolean }): Promise<AiHostStatus>;
  aiHostDisable(): Promise<AiHostStatus>;
  aiHostStatus(): Promise<AiHostStatus>;
  aiHostBeginPairing(): Promise<AiHostStatus>;
  aiHostRevoke(args: { deviceId: string }): Promise<AiHostStatus>;

  /* First-run AI setup (desktop). Install output streams on `aisetup:event`. */
  aiSetupStatus(): Promise<AiSetupStatus>;
  /** Step 1 of an install: the UI calls this only after the user accepts the consent dialog. */
  aiSetupConfirmInstall(args: { provider: AiSetupProviderId }): Promise<AiInstallConsent>;
  /** Step 2: runs the official install; refused without a valid one-time `consentToken`. */
  aiSetupInstall(args: { provider: AiSetupProviderId; consentToken: string }): Promise<void>;
  /** Shows the sign-in command; `openTerminal` opens a terminal with it where supported. Borealis never handles credentials. */
  aiSetupLogin(args: { provider: AiSetupProviderId; openTerminal?: boolean }): Promise<AiLoginHelp>;
  /** Remember that the first-run screen was finished (or skipped). */
  aiSetupComplete(args: { skipped?: boolean }): Promise<void>;

  /* Import anything; progress streams over `import:event` */
  importStart(args: ImportStartRequest): Promise<{ draftId: string }>;
  importGetDraft(args: { draftId: string }): Promise<ImportDraft>;
  importChooseParent(args: { draftId: string; classId: string; allowUntrustedWithCodex?: boolean }): Promise<ImportDraft>;
  /** Streams; `elementId` limits the instruction to one element (the per-element "fix with AI"). Resolves once the revision is accepted. */
  importRevise(args: { draftId: string; instruction: string; elementId?: string; allowUntrustedWithCodex?: boolean }): Promise<void>;
  /** Manual JSON edit; the element is re-validated and returned inside the draft. */
  importUpdateElement(args: { draftId: string; elementId: string; json: Record<string, unknown> }): Promise<ImportDraft>;
  /** Mark an element that needs checking as fine (or undo that). */
  importAcceptElement(args: { draftId: string; elementId: string; accepted: boolean }): Promise<ImportDraft>;
  /** Force an element to text only (true), to full rules (false), or back to automatic (null). */
  importSetTextOnly(args: { draftId: string; elementId: string; textOnly: boolean | null }): Promise<ImportDraft>;
  /** Streams like importRevise (ends with stage "ready" or "error"; then call importGetDraft): generate the element again from the given source lines (the generation path, not a revision). */
  importRelocateElement(args: { draftId: string; elementId: string; startLine: number; endLine: number; allowUntrustedWithCodex?: boolean }): Promise<void>;
  /** Streams like importRelocateElement: generate the element again from its current passage. The draft is unchanged when the model call fails. */
  importRegenerateElement(args: { draftId: string; elementId: string; allowUntrustedWithCodex?: boolean }): Promise<void>;
  importFinalize(args: { draftId: string } & ImportFinalizeRequest): Promise<ContentPack>;
  importDiscard(args: { draftId: string }): Promise<void>;
  /** Builds the opt-in bug report and keeps it for sending; sends nothing. */
  importReportPreview(args: ImportReportRequest): Promise<ImportReportPreview>;
  /** Sends exactly the previewed payload (HTTPS POST to the Borealis website). */
  importReportSend(args: { previewId: string }): Promise<ImportReportSent>;
}

export type CommandName = keyof ApiMethods;

/** Method name -> Tauri command name. Completeness enforced by the type. */
export const COMMANDS = {
  appVersion: "app_version",
  listCharacters: "list_characters",
  createCharacter: "create_character",
  loadCharacter: "load_character",
  getSnapshot: "get_snapshot",
  saveCharacter: "save_character",
  saveCharacterAs: "save_character_as",
  closeCharacter: "close_character",
  deleteCharacter: "delete_character",
  duplicateCharacter: "duplicate_character",
  updateCharacterMeta: "update_character_meta",
  setPortrait: "set_portrait",
  importCharacter: "import_character",
  recheckMissingContent: "recheck_missing_content",
  exportCharacter: "export_character",
  getSlotOptions: "get_slot_options",
  applySelection: "apply_selection",
  clearSelection: "clear_selection",
  replaceSpell: "replace_spell",
  setXp: "set_xp",
  levelUp: "level_up",
  levelDown: "level_down",
  addMulticlass: "add_multiclass",
  setHpRoll: "set_hp_roll",
  setAbilityMethod: "set_ability_method",
  setBaseScore: "set_base_score",
  rollAbilities: "roll_abilities",
  assignStandardArray: "assign_standard_array",
  resetAbilities: "reset_abilities",
  setCharacterOptions: "set_character_options",
  previewSourceRestrictions: "preview_source_restrictions",
  setSourceRestrictions: "set_source_restrictions",
  setAllowCrossEdition: "set_allow_cross_edition",
  getCompatibilityReport: "get_compatibility_report",
  getRulesetProfile: "get_ruleset_profile",
  setSpellPrepared: "set_spell_prepared",
  previewConvertTo2024: "preview_convert_to_2024",
  applyConvertTo2024: "apply_convert_to_2024",
  spendResource: "spend_resource",
  restoreResource: "restore_resource",
  shortRest: "short_rest",
  longRest: "long_rest",
  setToggle: "set_toggle",
  setExhaustion: "set_exhaustion",
  setHeroicInspiration: "set_heroic_inspiration",
  chooseWeaponMasteries: "choose_weapon_masteries",
  setWildShapeForms: "set_wild_shape_forms",
  chooseEquipmentPackage: "choose_equipment_package",
  searchItems: "search_items",
  addItem: "add_item",
  removeItem: "remove_item",
  equipItem: "equip_item",
  unequipItem: "unequip_item",
  setAttuned: "set_attuned",
  setRolledMax: "set_rolled_max",
  rerollMax: "reroll_max",
  storeItem: "store_item",
  moveItem: "move_item",
  updateItemDetails: "update_item_details",
  extractPack: "extract_pack",
  adjustCoins: "adjust_coins",
  convertCoins: "convert_coins",
  setStorageNames: "set_storage_names",
  explainStat: "explain_stat",
  updateDetails: "update_details",
  upsertAttack: "upsert_attack",
  removeAttack: "remove_attack",
  moveAttack: "move_attack",
  updateCompanion: "update_companion",
  renderSheet: "render_sheet",
  exportSheet: "export_sheet",
  searchCompendium: "search_compendium",
  getElement: "get_element",
  compendiumFacets: "compendium_facets",
  listPacks: "list_packs",
  importPack: "import_pack",
  previewImportPack: "preview_import_pack",
  installImportPreview: "install_import_preview",
  cancelImportPreview: "cancel_import_preview",
  takeOpenedFiles: "take_opened_files",
  setPackEnabled: "set_pack_enabled",
  removePack: "remove_pack",
  listSources: "list_sources",
  listIndexes: "list_indexes",
  addIndex: "add_index",
  removeIndex: "remove_index",
  updateIndexes: "update_indexes",
  clearDownloadedContent: "clear_downloaded_content",
  scan5etoolsFolder: "scan_5etools_folder",
  stage5etoolsZip: "stage_5etools_zip",
  stage5etoolsUrl: "stage_5etools_url",
  list5etoolsUrls: "list_5etools_urls",
  remove5etoolsUrl: "remove_5etools_url",
  update5etoolsUrl: "update_5etools_url",
  import5etools: "import_5etools",
  getSettings: "get_settings",
  saveSettings: "save_settings",
  pickPath: "pick_path",
  takePickedFiles: "take_picked_files",
  setAiOverride: "set_ai_override",
  getDataDirs: "get_data_dirs",
  setDocumentsDir: "set_documents_dir",
  setSecondaryContentDir: "set_secondary_content_dir",
  unlinkSecondaryContentDir: "unlink_secondary_content_dir",
  restoreDefaultSettings: "restore_default_settings",
  clearCaches: "clear_caches",
  contentUpdateStatus: "content_update_status",
  appUpdateStatus: "app_update_status",
  checkAppUpdate: "check_app_update",
  openDownloadPage: "open_download_page",
  openAppFolder: "open_app_folder",
  getRecentLogs: "get_recent_logs",
  restartApp: "restart_app",
  aiStatus: "ai_status",
  aiStartTask: "ai_start_task",
  aiCancelTask: "ai_cancel_task",
  aiValidateHomebrew: "ai_validate_homebrew",
  aiAddHomebrewToPack: "ai_add_homebrew_to_pack",
  applyPlan: "apply_plan",
  encounterEvaluate: "encounter_evaluate",
  encounterSuggest: "encounter_suggest",
  aiHostEnable: "ai_host_enable",
  aiHostDisable: "ai_host_disable",
  aiHostStatus: "ai_host_status",
  aiHostBeginPairing: "ai_host_begin_pairing",
  aiHostRevoke: "ai_host_revoke",
  aiSetupStatus: "ai_setup_status",
  aiSetupConfirmInstall: "ai_setup_confirm_install",
  aiSetupInstall: "ai_setup_install",
  aiSetupLogin: "ai_setup_login",
  aiSetupComplete: "ai_setup_complete",
  importStart: "import_start",
  importGetDraft: "import_get_draft",
  importChooseParent: "import_choose_parent",
  importRevise: "import_revise",
  importUpdateElement: "import_update_element",
  importAcceptElement: "import_accept_element",
  importSetTextOnly: "import_set_text_only",
  importRelocateElement: "import_relocate_element",
  importRegenerateElement: "import_regenerate_element",
  importFinalize: "import_finalize",
  importDiscard: "import_discard",
  importReportPreview: "import_report_preview",
  importReportSend: "import_report_send",
} as const satisfies Record<CommandName, string>;

/** Backend -> UI push events (Tauri `emit`). */
export interface ApiEvents {
  "ai:event": AiEvent;
  "aisetup:event": AiSetupEvent;
  "import:event": ImportEvent;
  "content:progress": ContentProgress;
  /** The "new version available" check finished. */
  "appupdate:status": AppUpdateStatus;
  /** 5etools import progress (`import_5etools_progress`). */
  "import5etools:progress": ContentProgress;
  /** Content store hot-reloaded: UI should refetch packs, catalogues and open slot options. */
  "content:reloaded": { reason: string };
  /** The OS handed the app a file; collect it with `takeOpenedFiles`. */
  "content:open-file": Record<string, never>;
  /** Files dropped on the window from the OS: tokens and names only; read them with `takePickedFiles`. */
  "files:dropped": FileDrop;
}
export type EventName = keyof ApiEvents;

/** Tauri event names for `ApiEvents`. */
export const EVENTS = {
  "ai:event": "borealis://ai-event",
  "aisetup:event": "borealis://ai-setup-event",
  "import:event": "borealis://import-event",
  "content:progress": "borealis://content-progress",
  "appupdate:status": "borealis://app-update",
  "import5etools:progress": "borealis://import_5etools_progress",
  "content:reloaded": "borealis://content-reloaded",
  "content:open-file": "borealis://open-file",
  "files:dropped": "borealis://file-drop",
} as const satisfies Record<EventName, string>;

export interface Api extends ApiMethods {
  subscribe<E extends EventName>(event: E, handler: (payload: ApiEvents[E]) => void): Unsubscribe;
  /** "mock" when no backend is attached (browser dev / tests). */
  readonly kind: "tauri" | "mock";
}

/** Convenience re-exports for UI code. */
export type { AiMessage, AiMode };
