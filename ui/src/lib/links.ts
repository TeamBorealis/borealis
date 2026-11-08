import type { Api } from "../api/contract";
import { setStatus } from "../state/status";
import { errorMessage } from "../state/useAsync";

/**
 * The app's own web pages (decision U15, GitHub #9). The backend's `open_link` opens exactly these addresses and refuses
 * any other (`src-tauri/src/links.rs` holds the same list; a Rust test keeps the two equal).
 */
export const LINKS = {
  website: "https://borealisbuilder.com/",
  news: "https://borealisbuilder.com/changelog",
  releases: "https://github.com/TeamBorealis/borealis/releases",
  support: "https://discord.gg/r4K4D3AJpZ",
  /** Help > Privacy Policy (B-86), on every platform including store builds. */
  privacy: "https://borealisbuilder.com/privacy",
  download: "https://borealisbuilder.com/#download",
  /** The website guides, each opened by the help button of the section it explains (decision U18, B-234). */
  guideCreateCharacter: "https://borealisbuilder.com/guides/create-a-character",
  guideImportAurora: "https://borealisbuilder.com/guides/import-aurora-content",
  guideImport5etools: "https://borealisbuilder.com/guides/import-5etools-data",
  guideMoveCharacters: "https://borealisbuilder.com/guides/move-characters-between-devices",
  guidePrintSheet: "https://borealisbuilder.com/guides/print-character-sheet",
} as const;

export const EXTERNAL_LINKS: readonly string[] = Object.values(LINKS);

/**
 * Open one of [`LINKS`] in the system browser through the backend. `window.open` is the fallback only when the backend
 * could not open it (no opener, an I/O failure); an address the backend refuses (`invalid_argument`) is never opened.
 */
export async function openLink(api: Pick<Api, "openLink">, url: string): Promise<void> {
  try {
    await api.openLink({ url });
  } catch (e) {
    const refused = typeof e === "object" && e !== null && "code" in e && (e as { code: unknown }).code === "invalid_argument";
    if (refused) { setStatus(errorMessage(e)); return; }
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
