import type { IconName } from "../Icon";
import { setStatus } from "../../state/status";

export interface MainTabDef {
  id: string;
  label: string;
  /** Short label for the phone bottom bar. */
  short: string;
  to: string;
  icon: IconName;
  needsCharacter: boolean;
  /** Path prefixes that keep this tab selected. */
  match: string[];
  /** On phones the tab lives under More. */
  secondary?: boolean;
}

/** Classic builder main tab order, plus PLAY (Borealis play mode). START also owns every non-character screen. */
export const MAIN_TABS: readonly MainTabDef[] = [
  { id: "start", label: "Start", short: "Start", to: "/characters", icon: "users", needsCharacter: false, match: ["/characters", "/compendium", "/search", "/content", "/import", "/ai", "/setup", "/settings", "/more"] },
  { id: "build", label: "Build", short: "Build", to: "/builder", icon: "hammer", needsCharacter: true, match: ["/builder"] },
  { id: "magic", label: "Magic", short: "Magic", to: "/spells", icon: "sparkles", needsCharacter: true, match: ["/spells"] },
  { id: "equipment", label: "Equipment", short: "Gear", to: "/equipment", icon: "backpack", needsCharacter: true, match: ["/equipment"] },
  { id: "manage", label: "Manage", short: "Manage", to: "/manage", icon: "pen", needsCharacter: true, match: ["/manage"], secondary: true },
  { id: "sheet", label: "Character Sheet", short: "Sheet", to: "/sheet", icon: "scroll", needsCharacter: true, match: ["/sheet"], secondary: true },
  { id: "play", label: "Play", short: "Play", to: "/play", icon: "shield", needsCharacter: true, match: ["/play"] },
];

/** Route-based sub-tabs for the START section (classic builder: CHARACTER COLLECTION | SOURCES | ADDITIONAL CONTENT). */
export const START_SUBNAV: readonly { to: string; label: string }[] = [
  { to: "/characters", label: "Character Collection" },
  { to: "/content/sources", label: "Sources" },
  { to: "/content", label: "Additional Content" },
];

/** Project website opened by the title-bar globe and the Help menu. Empty until the site exists. */
export const PROJECT_SITE_URL = "";

/** Opens the project website (optionally a sub-path); says so in the status bar while there is no site. */
export function openProjectSite(path = ""): void {
  if (PROJECT_SITE_URL) window.open(`${PROJECT_SITE_URL}${path}`, "_blank", "noopener,noreferrer");
  else setStatus("The project website is not published yet.");
}

export function activeTab(pathname: string): MainTabDef {
  return MAIN_TABS.find((t) => t.match.some((m) => pathname === m || pathname.startsWith(`${m}/`))) ?? MAIN_TABS[0]!;
}

export type ShellAction = "new" | "save" | "next" | "flow" | "exit" | "close" | "compendium" | "options" | "levelup" | "portraits" | "edition" | "settings" | "borealis-settings" | "send-to-device" | "receive-from-device";

export interface MenuEntry {
  id: string;
  label: string;
  shortcut?: string;
  to?: string;
  /** Project-website sub-path (opened externally). */
  site?: string;
  action?: ShellAction;
  separator?: boolean;
  needsCharacter?: boolean;
  checkable?: boolean;
  /** Shown only when the optional AI assistant is on. */
  ai?: boolean;
  /** Target while AI is off (the entry stays, but goes to the rules-based importer). */
  noAiTo?: string;
}
export interface MenuDef { id: string; label: string; entries: MenuEntry[] }

/** The classic builder's items first (same order), then a separator and the Borealis-only extras. */
export const MENUS: readonly MenuDef[] = [
  { id: "file", label: "File", entries: [
    { id: "new", label: "New Character", shortcut: "Ctrl+N", action: "new" },
    { id: "save", label: "Save", shortcut: "Ctrl+S", action: "save", needsCharacter: true },
    { id: "pdf", label: "Save Character Sheet (PDF)", to: "/sheet", needsCharacter: true },
    { id: "load", label: "Load Character", to: "/characters?load=1" },
    { id: "find", label: "Search Characters", to: "/characters?find=1" },
    { id: "import", label: "Import Characters", to: "/import", noAiTo: "/characters?load=1" },
    { id: "s1", label: "", separator: true },
    { id: "send", label: "Send to device...", action: "send-to-device" },
    { id: "receive", label: "Receive from device...", action: "receive-from-device" },
    { id: "s2", label: "", separator: true },
    { id: "exit", label: "Exit Application", action: "exit" },
  ] },
  { id: "view", label: "View", entries: [
    { id: "portraits", label: "Portraits Gallery", shortcut: "Ctrl+P", action: "portraits", needsCharacter: true },
    { id: "sheet", label: "Character Sheet View", shortcut: "Ctrl+V", to: "/sheet", needsCharacter: true },
    { id: "compendium", label: "Compendium", shortcut: "F1", action: "compendium" },
    { id: "next", label: "Next Selection", action: "next", needsCharacter: true },
    { id: "flow", label: "Enable Selection Flow", action: "flow", checkable: true },
    { id: "settings", label: "Application Settings", action: "settings" },
    { id: "borealis-settings", label: "Borealis Settings", action: "borealis-settings" },
    { id: "s1", label: "", separator: true },
    { id: "content", label: "Additional Content", to: "/content" },
    { id: "ai", label: "AI Assistant", to: "/ai", ai: true },
  ] },
  { id: "help", label: "Help", entries: [
    { id: "start", label: "Get Started", to: "/characters" },
    { id: "news", label: "News", site: "/news" },
    { id: "extra", label: "Additional Content", to: "/content" },
    { id: "releases", label: "Releases", site: "/releases" },
    { id: "support", label: "Support Page", site: "/support" },
    { id: "donate", label: "Support Borealis", site: "/support/donate" },
    { id: "s1", label: "", separator: true },
    { id: "setup", label: "AI Setup", to: "/setup/ai", ai: true },
    { id: "more", label: "All Screens", to: "/more" },
  ] },
];

/** The menus for the current AI state: AI entries appear only when AI is on, and the AI importer entry is hidden (Load Character covers it). */
export function visibleMenus(aiEnabled: boolean): readonly MenuDef[] {
  return MENUS.map((m) => ({
    ...m,
    entries: m.entries
      .filter((e) => aiEnabled || (!e.ai && !e.noAiTo)),
  }));
}

/** The loaded character's (gold) name menu: the classic builder's items in order, then Borealis extras after a separator. */
export const CHARACTER_MENU: readonly MenuEntry[] = [
  { id: "save", label: "Save", shortcut: "Ctrl+S", action: "save" },
  { id: "pdf", label: "Save Character Sheet (PDF)", to: "/sheet" },
  { id: "levels", label: "Level Up, HP & Multiclassing", action: "levelup" },
  { id: "options", label: "Manage Character Options", shortcut: "F8", action: "options" },
  { id: "sources", label: "Manage Sources", to: "/content/sources" },
  { id: "coins", label: "Manage Coins", to: "/equipment" },
  { id: "preview", label: "Update Sheet Preview", to: "/sheet" },
  { id: "s1", label: "", separator: true },
  { id: "manage", label: "Character Details", to: "/manage" },
  { id: "close", label: "Close Character", action: "close" },
  { id: "s2", label: "", separator: true },
  { id: "edition", label: "Edition & compatibility…", action: "edition" },
];

/** Every route label the shell can reach (used by the "every route is reachable" test). */
export function shellReachableTargets(): string[] {
  const targets = new Set<string>();
  MAIN_TABS.forEach((t) => targets.add(t.to));
  START_SUBNAV.forEach((t) => targets.add(t.to));
  targets.add("/compendium"); // View > Compendium and the search box open the flyout; the route is its deep link
  targets.add("/search"); // phones: More > Search opens the same flyout full screen
  [...MENUS.flatMap((m) => m.entries), ...CHARACTER_MENU].forEach((e) => e.to && targets.add(e.to));
  return [...targets];
}
