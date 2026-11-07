import { load } from "js-yaml";

/** A note's YAML front matter: a first line `---` and a later line `---` (the backend's `syntax::front_matter` rule). */
export interface FrontMatter {
  /** The YAML between the `---` lines. */
  yaml: string;
  /** Offset just after the closing line (and its line break). */
  end: number;
  /** 0-based index of the first line after the block. */
  endLine: number;
}

export function splitFrontMatter(text: string): FrontMatter | null {
  const lines = text.split("\n");
  if (lines.length < 2 || lines[0]!.trimEnd() !== "---") return null;
  let offset = lines[0]!.length + 1;
  for (let i = 1; i < lines.length; i += 1) {
    const line = lines[i]!;
    if (line.trimEnd() === "---") {
      const end = offset + line.length + (i < lines.length - 1 ? 1 : 0);
      return { yaml: lines.slice(1, i).join("\n"), end, endLine: i + 1 };
    }
    offset += line.length + 1;
  }
  return null;
}

export interface Properties {
  /** Name and value pairs in file order. */
  entries: [string, unknown][];
  /** Why the YAML could not be shown as properties, or null. */
  error: string | null;
}

/** Read front matter YAML as properties (YAML 1.2 core schema: dates stay text, as Obsidian shows them). */
export function parseProperties(yaml: string): Properties {
  try {
    const value = load(yaml);
    if (value === null || value === undefined) return { entries: [], error: null };
    if (typeof value !== "object" || Array.isArray(value)) return { entries: [], error: "The properties are not a list of names and values." };
    return { entries: Object.entries(value as Record<string, unknown>), error: null };
  } catch {
    return { entries: [], error: "The properties could not be read. They are kept as they are in the note." };
  }
}

/** The tags a `tags` / `tag` property lists (a list, or a comma or space separated text). */
export function propertyTags(value: unknown): string[] {
  const words = Array.isArray(value) ? value.map(String) : typeof value === "string" || typeof value === "number" ? String(value).split(/[,\s]+/) : [];
  return words.map((w) => w.trim().replace(/^#/, "")).filter(Boolean);
}
