import { useEffect } from "react";
import type { DataDirs } from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";

/** Warning for a chosen documents directory that cannot be used this session. */
export function documentsUnavailableText(path: string): string {
  return `The documents directory ${path} is not available. Characters are loaded from and saved to the default folder until it is back.`;
}

export const SECONDARY_UNAVAILABLE =
  "The secondary content directory is not available. Its content stays installed, and Borealis reads the folder again at the next start once it is back.";

export const PORTRAIT_LIBRARY_UNAVAILABLE =
  "The portrait library folder is not available. Its images are shown again in the portraits gallery once it is back.";

export const VAULT_UNAVAILABLE =
  "The Obsidian vault folder is not available. Its notes show again in Notes once it is back.";

/** Status-bar text for chosen folders that cannot be reached (unplugged drive, offline share), or null when all are fine. */
export function describeFolders(dirs: DataDirs): string | null {
  const parts = [
    dirs.documentsDirUnavailable ? documentsUnavailableText(dirs.documentsDirUnavailable) : null,
    dirs.secondaryContentUnavailable ? SECONDARY_UNAVAILABLE : null,
    dirs.portraitLibraryUnavailable ? PORTRAIT_LIBRARY_UNAVAILABLE : null,
    dirs.vaultUnavailable ? VAULT_UNAVAILABLE : null,
  ].filter((p): p is string => p !== null);
  return parts.length ? parts.join(" ") : null;
}

/** At startup, tell the user in the status bar when a chosen folder is unavailable (non-blocking; nothing is forgotten or removed). */
export function useFolderNotice(): void {
  const api = useApi();
  useEffect(() => {
    let alive = true;
    api.getDataDirs().then((dirs) => {
      const text = alive ? describeFolders(dirs) : null;
      if (text) setStatus(text);
    }, () => undefined);
    return () => { alive = false; };
  }, [api]);
}
