import type { Api } from "../api/contract";

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

/** Open one of [`LINKS`] in the system browser through the backend; `window.open` only when the backend cannot. */
export async function openLink(api: Pick<Api, "openLink">, url: string): Promise<void> {
  try { await api.openLink({ url }); } catch { window.open(url, "_blank", "noopener,noreferrer"); }
}
