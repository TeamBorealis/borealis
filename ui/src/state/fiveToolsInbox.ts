/**
 * 5etools-format homebrew files handed to another importer (the "+" button, a drop, "open with", Custom content) are
 * parked here for the 5etools-format import, which takes them when it mounts or when new ones arrive (B-238).
 */
let pending: File[] = [];

/** What the user is told when files are sent to the 5etools-format import. */
export function routedMessage(names: readonly string[]): string {
  const list = names.join(", ");
  return names.length === 1
    ? `${list} is a 5etools-format homebrew file. It was opened in the 5etools-format import.`
    : `${list} are 5etools-format homebrew files. They were opened in the 5etools-format import.`;
}
const listeners = new Set<() => void>();

export function sendToFiveTools(files: File[]): void {
  if (files.length === 0) return;
  pending = [...pending, ...files];
  listeners.forEach((l) => l());
}

export function takeFiveToolsFiles(): File[] {
  const out = pending;
  pending = [];
  return out;
}

/** Calls `listener` whenever files are parked; returns the unsubscribe function. */
export function onFiveToolsFiles(listener: () => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}
