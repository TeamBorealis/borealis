import type { VaultEntryKind } from "../../api/contract";

/** An MDI glyph at the app's icon size. */
export function Svg({ d }: { d: string }) {
  return <svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={d} fill="currentColor" /></svg>;
}

/** The last part of a vault path. */
export const baseName = (path: string): string => path.split("/").pop() ?? path;

/** The folder part of a vault path ("" for the vault folder). */
export const folderOf = (path: string): string => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");

/** How Obsidian shows a file: notes without `.md`, everything else with its extension. */
export function displayName(name: string, kind: VaultEntryKind | null = null): string {
  return kind === "note" || (kind === null && /\.md$/i.test(name)) ? name.replace(/\.md$/i, "") : name;
}

/** The name a note or file is renamed from (no extension: the backend keeps it). */
export function renameStem(path: string): string {
  const name = baseName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}
