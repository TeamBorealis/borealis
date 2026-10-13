import { readBuffer } from "./files";

/** What a file handed to Import ("+" button, window-wide drop) is, decided by name and, for .json, by content. */
export type ImportKind = "pack" | "character" | "other";

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

/** XML, .index and .zip go to the pack importer, .dnd5e to the character importer, .json by sniffing; the rest is "other". */
export async function classifyImport(file: File): Promise<ImportKind> {
  switch (ext(file.name)) {
    case "xml":
    case "index":
    case "zip":
      return "pack";
    case "dnd5e":
      return "character";
    case "json":
      return looksLikePack(new TextDecoder().decode(await readBuffer(file))) ? "pack" : "character";
    default:
      return "other";
  }
}

/** The neutral message for a file the rules-based importers cannot read. */
export function unsupportedImportMessage(names: readonly string[], mobile: boolean): string {
  const list = names.join(", ");
  return mobile
    ? `${list} cannot be imported here. Use an .xml, .index, .zip, .dnd5e or .json file.`
    : `${list} cannot be imported directly. PDFs, photos and plain text need the optional AI assistant (turn it on in Borealis Settings). You can also use an .xml, .index, .zip, .dnd5e or .json file.`;
}
