import type { ContentPack, PreviewBook, Ruleset } from "../types";
import { apiError } from "./errors";

/**
 * IM14 (GitHub #11) in the mock host: the importer's edition guess for a book and the user's stored choice per book (by
 * name, lower-cased, apostrophes folded, like `borealis_content::source_key`). The real backend re-tags the book's
 * elements when content loads; the mock only reports the edition.
 */
export const bookKey = (name: string): string => name.trim().replace(/’/g, "'").toLowerCase();

const PHB_2024_RELEASE = "20240917";

/** The importer's rule (`borealis_migrate::edition::is_2024_source`). */
export function guessEdition(name: string, setters: Record<string, string>): Ruleset {
  if ((setters.information ?? "").replace("&amp;", "&").startsWith("D&D 5.5e")) return "dnd2024";
  const years = (name.match(/\d+/g) ?? []).filter((w) => w.length === 4).map(Number);
  if (years.includes(2014)) return "dnd2014";
  if (name.includes("5.5") || years.some((y) => y >= 2024 && y < 2100)) return "dnd2024";
  const release = /^\d{8}$/.test(setters.release ?? "") ? setters.release! : null;
  return release !== null && release >= PHB_2024_RELEASE ? "dnd2024" : "dnd2014";
}

/** The `Source` elements of some Aurora XML texts with the importer's edition for each, each name once. */
export function booksOf(texts: string[]): { name: string; detected: Ruleset }[] {
  const out: { name: string; detected: Ruleset }[] = [];
  for (const text of texts) {
    for (const m of text.matchAll(/<element\b([^>]*)>([\s\S]*?)<\/element>/g)) {
      const head = m[1] ?? "";
      if (!/\btype="Source"/.test(head)) continue;
      const name = /\bname="([^"]*)"/.exec(head)?.[1];
      if (!name || out.some((b) => bookKey(b.name) === bookKey(name))) continue;
      const setters: Record<string, string> = {};
      for (const s of (m[2] ?? "").matchAll(/<set name="([^"]+)"[^>]*>([^<]*)<\/set>/g)) setters[s[1]!.toLowerCase()] = s[2]!.trim();
      out.push({ name, detected: guessEdition(name, setters) });
    }
  }
  return out;
}

/** The user's stored editions. */
export class MockBookEditions {
  private chosen = new Map<string, Ruleset>();

  get(name: string): Ruleset | undefined { return this.chosen.get(bookKey(name)); }

  set(name: string, edition: Ruleset): void { this.chosen.set(bookKey(name), edition); }

  previewBooks(texts: string[]): PreviewBook[] {
    return booksOf(texts).map(({ name, detected }) => {
      const set = this.get(name);
      return { name, detected, edition: set ?? detected, setByUser: set !== undefined };
    });
  }

  /** A source row's edition fields. */
  fields(name: string, pack: ContentPack): { edition: Ruleset | null; editionSetByUser: boolean; editionEditable: boolean } {
    const set = this.get(name);
    const own = pack.rulesets.length === 1 ? pack.rulesets[0]! : null;
    return { edition: set ?? own, editionSetByUser: set !== undefined, editionEditable: pack.kind !== "bundled" };
  }
}

/** The edition the user may set: only 2014 or 2024, only on imported books. */
export function checkSettable(editable: boolean | undefined, edition: Ruleset): void {
  if (edition !== "dnd2014" && edition !== "dnd2024") throw apiError("invalid_argument", "A book is either 2014 or 2024 rules.");
  if (editable === undefined) throw apiError("not_found", "That book is not installed.");
  if (!editable) throw apiError("invalid_argument", "Bundled books keep their edition; only imported books can be set.");
}
