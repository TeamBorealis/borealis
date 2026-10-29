/** Bidi formatting and zero-width characters: they could show a tracker name reversed or hide text (GitHub #2 review L4). */
const HIDDEN = /[\u061C\u200B-\u200F\u202A-\u202E\u2060-\u2069\uFEFF]/g;

/** A tracker name as the host keeps it: hidden formatting characters dropped, then trimmed. */
export const cleanTrackerName = (name: string): string => name.replace(HIDDEN, "").trim();
