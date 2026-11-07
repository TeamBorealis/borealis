/**
 * The one settings window's tabs (GitHub #31). The classic builder's own tabs keep its names and order; the Borealis-only
 * tabs sit where the classic builder puts its developer-mode tab (after CHARACTER SHEET, before LICENSE and ABOUT), so the
 * classic tabs stay first and LICENSE / ABOUT stay last.
 */
export interface SettingsTabDef {
  id: string;
  label: string;
  /** Borealis-only (not in the classic builder's window). */
  borealis?: boolean;
  /** 20 px right padding (the classic builder's GENERAL, CHARACTER BUILD and DATA FILES); the others use 10 px. */
  wide?: boolean;
  /** Only in builds with the AI CLI integration (`aiAvailable`: desktop, not the Mac App Store). */
  ai?: boolean;
  /** Only while the AI assistant is on (saved settings). */
  aiOn?: boolean;
  /** Not in store builds (iOS, Google Play, Mac App Store): they get updates from the store. */
  directOnly?: boolean;
}

export const SETTINGS_TABS: readonly SettingsTabDef[] = [
  { id: "general", label: "General", wide: true },
  { id: "appearance", label: "Appearance" },
  { id: "build", label: "Character build", wide: true },
  { id: "data", label: "Data files", wide: true },
  { id: "equipment", label: "Equipment" },
  { id: "sheet", label: "Character sheet" },
  { id: "rules", label: "Rules", borealis: true, wide: true },
  { id: "saving", label: "Saving", borealis: true, wide: true },
  { id: "display", label: "Display", borealis: true, wide: true },
  { id: "content", label: "Content", borealis: true },
  { id: "search", label: "Search", borealis: true },
  { id: "ai", label: "AI assistant", borealis: true, ai: true },
  { id: "phones", label: "Phones", borealis: true, ai: true, aiOn: true },
  { id: "devices", label: "Devices", borealis: true },
  { id: "updates", label: "Updates", borealis: true, wide: true, directOnly: true },
  { id: "license", label: "License" },
  { id: "about", label: "About" },
];

export interface TabGates { aiAvailable: boolean; aiEnabled: boolean; storeBuild: boolean }

/** The tabs this build and the saved settings show. */
export function visibleSettingsTabs({ aiAvailable, aiEnabled, storeBuild }: TabGates): SettingsTabDef[] {
  return SETTINGS_TABS.filter((t) => (!t.ai || aiAvailable) && (!t.aiOn || aiEnabled) && !(t.directOnly && storeBuild));
}

/** The tab to show for a requested id: the request when it is visible here, else GENERAL. */
export function resolveSettingsTab(requested: string | undefined, tabs: readonly SettingsTabDef[]): string {
  return tabs.some((t) => t.id === requested) ? requested! : "general";
}
