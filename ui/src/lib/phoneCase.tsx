import { usePhone } from "./phone";

/** Words that stay in capitals in sentence case: abbreviations the rules and the app use as such. */
const KEEP = new Set(["AI", "AC", "HP", "XP", "DC", "CR", "STR", "DEX", "CON", "INT", "WIS", "CHA", "PP", "GP", "EP", "SP", "CP", "PDF", "QR", "SRD", "ID", "UI", "URL", "MCP", "LAN"]);
/** Names that keep their capital letter. */
const NAMES = new Set(["BOREALIS", "AURORA"]);

/**
 * B-48 (spec sec. 3.3): the same words in sentence case. Only an all-capitals label changes ("PLAYER NAME" -> "Player name");
 * abbreviations (AI, XP, STR) and names (Borealis) keep their capitals, and text that already has lower case is left alone.
 */
export function sentenceCase(text: string): string {
  if (!/[A-Z]/.test(text) || /[a-z]/.test(text)) return text;
  let first = true;
  return text.replace(/[A-Z][A-Z'’]*/g, (word) => {
    const out = KEEP.has(word) ? word : NAMES.has(word) || first ? word[0] + word.slice(1).toLowerCase() : word.toLowerCase();
    first = false;
    return out;
  });
}

/** Renders an all-capitals label in sentence case on phones; desktop keeps Aurora's exact capitals (U7). */
export function PhoneCase({ children }: { children: string }) {
  return <>{usePhone() ? sentenceCase(children) : children}</>;
}
