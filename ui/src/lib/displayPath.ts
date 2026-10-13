/**
 * Privacy-safe form of a file path for toasts and the status bar: the user's home folder
 * (`/Users/<name>`, `/home/<name>`, `C:\Users\<name>`) becomes `~`, so the account name never shows on screen
 * (screen recordings, screenshots, shared screens). Paths outside a home folder are returned unchanged.
 * The status bar shows the full path; this keeps the path, minus the home prefix.
 */
const HOME_PATTERNS: readonly RegExp[] = [
  /^\/Users\/(?!Shared(?:\/|$))[^/]+(?=\/|$)/,
  /^\/(?:var\/)?home\/[^/]+(?=\/|$)/,
  /^\/root(?=\/|$)/,
  /^[A-Za-z]:[\\/]Users[\\/][^\\/]+(?=[\\/]|$)/i,
];

export function displayPath(path: string): string {
  const trimmed = path.trim();
  for (const home of HOME_PATTERNS) {
    if (home.test(trimmed)) return trimmed.replace(home, "~");
  }
  return trimmed;
}
