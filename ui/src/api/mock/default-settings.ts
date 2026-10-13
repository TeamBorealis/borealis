import type { AppSettings } from "../types";

export const DEFAULT_SETTINGS: AppSettings = {
  playerName: "Player One", defaultRuleset: "dnd2024", abilityMethod: "roll_4d6", pointBuyBudget: 27, abilityMinBase: 3, abilityMaxBase: 30,
  abilityCapEnabled: true, selectionFlow: false, confirmRemoveLevel: true, confirmEquipWeaponNotInAttacks: true, density: "comfortable",
  theme: "system", highContrast: false, tileSize: 160, autosave: true,
  sheet: {
    formFillable: false, theme: "colour", edition: "auto", styleFormatting: true, swapAbilityPositions: false, includeNonPreparedSpells: false,
    cards: { spell: false, item: false, attack: false, feature: false }, autoGenerate: true, openAfterSave: true,
    cardsNewPage: { item: true, attack: false, feature: true }, legacySpellcastingPage: false,
  },
  ai: { enabled: false, provider: "claude", pairedHost: null, overridePath: null },
  documentsDir: null, secondaryContentDir: null, autoContentUpdate: true, rowHeight: "medium",
  skillsPanelExpanded: true, savingThrowsExpanded: false, accentColor: "default", portraitSize: 150, showImportNotice: true, mobile: false,
};
