import { readBuffer } from "./files";

/** What a file handed to Import ("+" button, window-wide drop) is, decided by name and, for .json, by content.
 *  `fivetools`: 5etools-format homebrew or data, which the 5etools-format import reads (B-238). */
export type ImportKind = "pack" | "character" | "fivetools" | "other";

/** Extensions the rules-based importers read. Everything else needs the optional AI assistant (desktop only). */
export const RULES_IMPORT_ACCEPT = ".xml,.index,.zip,.dnd5e,.json,application/json,text/xml,application/zip";

const ext = (name: string) => name.toLowerCase().split(".").pop() ?? "";

/** A native character file has no content-format marker; a content pack does (or lists elements). */
function looksLikePack(text: string): boolean {
  try {
    const v: unknown = JSON.parse(text);
    if (!v || typeof v !== "object") return false;
    const o = v as Record<string, unknown>;
    return (typeof o.format === "string" && /content/i.test(o.format)) || Array.isArray(o.elements);
  } catch {
    return false;
  }
}

/** Entity arrays only 5etools-format data has (the importer reads more; these are enough to recognise a file). */
const FIVETOOLS_ARRAYS = ["spell", "item", "baseitem", "magicvariant", "monster", "class", "subclass", "classFeature", "subclassFeature",
  "race", "subrace", "feat", "background", "optionalfeature", "language", "condition"];

/** A 5etools-format homebrew or data file: a `_meta` block or 5etools-format entity arrays, and no Borealis `elements`. */
export function looksLikeFiveTools(text: string): boolean {
  try {
    const v: unknown = JSON.parse(text);
    if (!v || typeof v !== "object" || Array.isArray(v)) return false;
    const o = v as Record<string, unknown>;
    if ("elements" in o) return false;
    const meta = o._meta;
    return (!!meta && typeof meta === "object" && !Array.isArray(meta)) || FIVETOOLS_ARRAYS.some((k) => Array.isArray(o[k]));
  } catch {
    return false;
  }
}

/** True for a `.json` file whose content is 5etools-format data (TextDecoder drops a byte order mark). */
export async function isFiveToolsFile(file: File): Promise<boolean> {
  return ext(file.name) === "json" && looksLikeFiveTools(new TextDecoder().decode(await readBuffer(file)));
}

/** XML, .index and .zip go to the pack importer, .dnd5e to the character importer, .json by sniffing (a 5etools-format
 *  file to the 5etools-format import); the rest is "other". */
export async function classifyImport(file: File): Promise<ImportKind> {
  switch (ext(file.name)) {
    case "xml":
    case "index":
    case "zip":
      return "pack";
    case "dnd5e":
      return "character";
    case "json": {
      const text = new TextDecoder().decode(await readBuffer(file));
      if (looksLikePack(text)) return "pack";
      return looksLikeFiveTools(text) ? "fivetools" : "character";
    }
    default:
      return "other";
  }
}

/** The neutral message for a file the rules-based importers cannot read. Only a build with the AI CLI integration
 * (`aiAvailable`) mentions the AI assistant. */
export function unsupportedImportMessage(names: readonly string[], aiAvailable: boolean): string {
  const list = names.join(", ");
  return !aiAvailable
    ? `${list} cannot be imported here. Use an .xml, .index, .zip, .dnd5e or .json file.`
    : `${list} cannot be imported directly. PDFs, photos and plain text need the optional AI assistant (turn it on in Application Settings > AI Assistant). You can also use an .xml, .index, .zip, .dnd5e or .json file.`;
}
