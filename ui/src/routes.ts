import type { IconName } from "./components/Icon";

export interface RouteDef {
  readonly path: string;
  readonly label: string;
  readonly icon: IconName;
  /** Needs an open character (shows the character bar). */
  readonly needsCharacter: boolean;
  /** Not in the phone bottom bar (lives under "More"). */
  readonly secondary: boolean;
  /** Only when the optional AI assistant is on (never on mobile). */
  readonly ai?: boolean;
}

export const ROUTES: readonly RouteDef[] = [
  { path: "/characters", label: "Characters", icon: "users", needsCharacter: false, secondary: false },
  { path: "/builder", label: "Build", icon: "hammer", needsCharacter: true, secondary: false },
  { path: "/play", label: "Play", icon: "shield", needsCharacter: true, secondary: false },
  { path: "/spells", label: "Magic", icon: "sparkles", needsCharacter: true, secondary: false },
  { path: "/equipment", label: "Gear", icon: "backpack", needsCharacter: true, secondary: false },
  { path: "/manage", label: "Manage", icon: "pen", needsCharacter: true, secondary: true },
  { path: "/sheet", label: "Sheet", icon: "scroll", needsCharacter: true, secondary: true },
  { path: "/compendium", label: "Compendium", icon: "book", needsCharacter: false, secondary: true },
  { path: "/content", label: "Content", icon: "box", needsCharacter: false, secondary: true },
  { path: "/import", label: "Import", icon: "upload", needsCharacter: false, secondary: true, ai: true },
  { path: "/ai", label: "AI Assistant", icon: "bot", needsCharacter: false, secondary: true, ai: true },
];
