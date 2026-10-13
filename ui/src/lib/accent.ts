import type { AccentColor } from "../api/contract";

/** Accent swatches in the classic builder's dialog order; the colours are Borealis's own. */
export const ACCENTS: readonly { id: AccentColor; label: string; color: string }[] = [
  { id: "black", label: "Black", color: "#111111" },
  { id: "brown", label: "Brown", color: "#5d4037" },
  { id: "default", label: "Default", color: "#293338" },
  { id: "purple", label: "Purple", color: "#5b3a7a" },
  { id: "green", label: "Green", color: "#2e6b3e" },
  { id: "aqua", label: "Aqua", color: "#1f6f7a" },
  { id: "blue", label: "Blue", color: "#2a4f8a" },
  { id: "mauve", label: "Mauve", color: "#7a5a6b" },
  { id: "pink", label: "Pink", color: "#a8456f" },
  { id: "red", label: "Red", color: "#8e2a2a" },
];

const ACCENT_VARS = ["--accent-base", "--accent-80", "--accent-60", "--accent-40", "--accent-20", "--highlight"] as const;

/** Re-skins the app with an accent colour; "default" removes the overrides so the stylesheet's own accent applies. */
export function applyAccent(id: AccentColor): void {
  const root = document.documentElement;
  const found = ACCENTS.find((a) => a.id === id);
  if (!found || id === "default") {
    for (const v of ACCENT_VARS) root.style.removeProperty(v);
    return;
  }
  const c = found.color;
  root.style.setProperty("--accent-base", c);
  root.style.setProperty("--accent-80", `${c}cc`);
  root.style.setProperty("--accent-60", `${c}99`);
  root.style.setProperty("--accent-40", `${c}66`);
  root.style.setProperty("--accent-20", `${c}33`);
  root.style.setProperty("--highlight", c);
}
