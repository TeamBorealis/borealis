import type { SheetPaper } from "../api/types/settings";

/**
 * Regions that print on US Letter (CLDR's US-Letter paper territories); everywhere else uses A4. TV1, B-147: the Paper
 * setting defaults by the device's region.
 */
export const LETTER_REGIONS: ReadonlySet<string> = new Set(["BZ", "CA", "CL", "CO", "CR", "GT", "MX", "NI", "PA", "PH", "PR", "SV", "US", "VE"]);

/** The region of a BCP 47 tag ("en-US" gives "US"); a tag without one gets its likely region ("he" gives "IL"). */
function regionOf(tag: string): string | null {
  try {
    const locale = new Intl.Locale(tag);
    return locale.region ?? locale.maximize().region ?? null;
  } catch {
    return null;
  }
}

/** The paper of the device's region: the first preferred language that names or implies a region decides; A4 when none does. */
export function regionPaper(languages: readonly string[]): SheetPaper {
  for (const tag of languages) {
    const region = regionOf(tag);
    if (region) return LETTER_REGIONS.has(region) ? "letter" : "a4";
  }
  return "a4";
}

/** The device's preferred languages, as the web view reports them. */
export function deviceLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : [navigator.language].filter(Boolean);
}

/** The paper the sheet prints on: the user's choice, else the device region's. */
export function sheetPaper(chosen: SheetPaper | null | undefined, languages: readonly string[] = deviceLanguages()): SheetPaper {
  return chosen ?? regionPaper(languages);
}
