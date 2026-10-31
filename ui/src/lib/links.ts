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
  download: "https://borealisbuilder.com/#download",
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
