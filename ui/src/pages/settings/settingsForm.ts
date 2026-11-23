import type { AppSettings } from "../../api/contract";

/**
 * The fields the settings form holds until Save (the classic builder's deferred apply). Everything else is taken from the
 * stored settings on Save: folders (saved through their own token calls), the AI program path (set at once by Browse /
 * Clear), the search settings (the Search tab acts at once, like the classic builder's cache button) and the read-only
 * platform flags.
 */
export const FORM_FIELDS: readonly (keyof AppSettings)[] = [
  // Classic builder tabs.
  "playerName", "abilityMethod", "rowHeight", "selectionFlow", "skillsPanelExpanded", "savingThrowsExpanded", "confirmRemoveLevel",
  "theme", "accentColor", "portraitSize", "autoContentUpdate", "showImportNotice", "abilityCapEnabled", "confirmEquipWeaponNotInAttacks",
  // Borealis tabs: RULES, SAVING, DISPLAY, UPDATES.
  "defaultRuleset", "pointBuyBudget", "abilityMinBase", "abilityMaxBase", "autosave", "highContrast", "density", "checkAppUpdates",
];
export const SHEET_FORM_FIELDS: readonly (keyof AppSettings["sheet"])[] = [
  "formFillable", "theme", "edition", "styleFormatting", "swapAbilityPositions", "includeNonPreparedSpells", "cards", "cardsNewPage",
  "legacySpellcastingPage", "paper", "autoGenerate", "openAfterSave",
];
/** AI ASSISTANT: the switch and the provider wait for Save; the program path is set at once. */
export const AI_FORM_FIELDS: readonly (keyof AppSettings["ai"])[] = ["enabled", "provider"];

function pick<T extends object>(stored: T, form: T, keys: readonly (keyof T)[]): T {
  const next = { ...stored };
  for (const k of keys) if (k in form) next[k] = form[k];
  return next;
}

/** The full settings to send on Save: the form's own fields over the stored settings. */
export function mergeForm(stored: AppSettings, form: AppSettings): AppSettings {
  return {
    ...pick(stored, form, FORM_FIELDS),
    sheet: pick(stored.sheet, form.sheet, SHEET_FORM_FIELDS),
    ai: pick(stored.ai, form.ai, AI_FORM_FIELDS),
  };
}

/** True when Save would change something (`held` = folders picked but not saved yet). */
export function formChanged(stored: AppSettings, form: AppSettings, held: boolean): boolean {
  return held || JSON.stringify(mergeForm(stored, form)) !== JSON.stringify(stored);
}
