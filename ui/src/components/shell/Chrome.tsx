import { type FormEvent, useEffect, useRef, useState } from "react";
import { NavLink, useLocation, useNavigate } from "react-router";
import { Portrait } from "../Portrait";
import { Icon } from "../Icon";
import { MetroProgressBar } from "../controls/Controls";
import { ToolTip } from "../controls/Controls";
import { useStatusMessage, setStatus } from "../../state/status";
import { MAIN_TABS, START_SUBNAV, activeTab, openProjectSite } from "./sections";
import { isTauri } from "../../lib/tauri";
import "./flyouts.css";
import { useShell } from "./ShellContext";
import { QuickViewFlyout } from "./QuickViewFlyout";
import { UpdateCommand } from "./UpdateFlyout";
import { PortraitsFlyout } from "../../pages/manage/PortraitsFlyout";
import { useCharacter } from "../../state/CharacterContext";
import { useShellActions } from "./useShellActions";
import { displayPath } from "../../lib/displayPath";
import { usePhone } from "../../lib/phone";

/** Quick search box (Alt+Q focuses it). Enter opens the compendium with the query. */
export function SearchBox() {
  const { openCompendium } = useShell();
  const [q, setQ] = useState("");
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const key = (e: KeyboardEvent) => { if (e.altKey && e.key.toLowerCase() === "q") { e.preventDefault(); ref.current?.focus(); } };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  const submit = (e: FormEvent) => { e.preventDefault(); if (q.trim()) openCompendium({ query: q.trim() }); };
  return (
    <form className="aur-search" role="search" onSubmit={submit}>
      <Icon name="book-search" />
      <input ref={ref} type="search" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Compendium Search (Alt+Q)" aria-label="Compendium search" />
    </form>
  );
}

/** Title-bar command boxes: open character sheet (only with a character), settings, project website (globe). */
export function TitleCommands() {
  const nav = useNavigate();
  const { openSettings } = useShell();
  const { snapshot } = useShellActions();
  const openSite = () => openProjectSite();
  return (
    <div className="aur-wincmds">
      {snapshot && <ToolTip text="Character sheet"><button type="button" aria-label="Character sheet" onClick={() => nav("/sheet")}><Icon name="open-in-app" /></button></ToolTip>}
      <ToolTip text="Settings"><button type="button" aria-label="Settings" onClick={() => openSettings()}><Icon name="cog" /></button></ToolTip>
      <UpdateCommand />
      <ToolTip text="Project website"><button type="button" aria-label="Project website" onClick={openSite}><Icon name="globe" /></button></ToolTip>
    </div>
  );
}

/** Row 1 main tab strip (18px SemiBold, accent underline + pointer under the selected tab). */
export function MainTabs() {
  const { pathname } = useLocation();
  const { snapshot } = useShellActions();
  const active = activeTab(pathname);
  return (
    <nav className="tabs main" aria-label="Main">
      {MAIN_TABS.map((t) => {
        const disabled = t.needsCharacter && !snapshot;
        if (disabled) return <span key={t.id} className="tab disabled" aria-disabled="true">{t.label}</span>;
        return (
          <NavLink key={t.id} to={t.to} className="tab" data-testid={`nav-${t.id}`} aria-current={t.id === active.id ? "page" : undefined} data-selected={t.id === active.id}>
            {t.label}
          </NavLink>
        );
      })}
    </nav>
  );
}

/** Character profile block (right of the tab row): name, build line, XP line, portrait; a ringed plus button when nothing is open. */
export function ProfileBlock() {
  const nav = useNavigate();
  const { snapshot } = useShellActions();
  const { mutate } = useCharacter();
  const { portraitsOpen, setPortraitsOpen } = useShell();
  const [quickOpen, setQuickOpen] = useState(false);
  if (!snapshot) {
    return <button type="button" className="aur-newchar" aria-label="New character" onClick={() => nav("/characters?new=1")}><Icon name="plus" /></button>;
  }
  const classes = snapshot.level.classes.map((c) => c.name).filter(Boolean).join(" / ");
  const race = snapshot.slots.find((x) => x.tab === "race" && x.chosen)?.chosen?.name ?? "";
  const { xp, currentLevelXp, nextLevelXp } = snapshot.level;
  const span = nextLevelXp === null ? 0 : nextLevelXp - currentLevelXp;
  const pct = span > 0 ? Math.max(0, Math.min(100, ((xp - currentLevelXp) / span) * 100)) : 0;
  const openQuickView = () => setQuickOpen(true);
  const openPortraits = () => setPortraitsOpen(true);
  return (
    <div className="aur-profile">
      {quickOpen && <QuickViewFlyout snapshot={snapshot} onClose={() => setQuickOpen(false)} />}
      {portraitsOpen && <PortraitsFlyout current={snapshot.portrait} onClose={() => setPortraitsOpen(false)}
        onSelect={(dataUrl) => void mutate((api, id) => api.setPortrait({ characterId: id, dataUrl }))} />}
      <div className="aur-profile-text">
        <button type="button" className="name" onClick={openQuickView}>{snapshot.name}</button>
        <div className="build">Level {snapshot.level.total} {[race, classes].filter(Boolean).join(" ")}</div>
        <div className="aur-xp" role="progressbar" aria-label="Experience" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}><i style={{ width: `${pct}%` }} /></div>
      </div>
      <button type="button" className="aur-portrait-btn" aria-label="Open portraits" onClick={openPortraits}>
        <Portrait src={snapshot.portrait} name={snapshot.name} className="aur-portrait" />
      </button>
    </div>
  );
}

/** START screens outside the collection show the classic builder's single-sub-tab pattern (one active label). */
const SINGLE_SUBTABS: readonly { to: string; label: string }[] = [
  { to: "/import", label: "Import" },
  { to: "/ai", label: "AI Assistant" },
];

/** Row 2: the sub-tab band. Route-based sub-tabs for START; other sections portal their strip into `host`. */
export function SubBand() {
  const { pathname } = useLocation();
  const { setHost } = useShell();
  const start = activeTab(pathname).id === "start";
  // Longest matching sub-tab wins so /content/sources does not also light up Additional Content.
  const single = SINGLE_SUBTABS.find((s) => pathname === s.to || pathname.startsWith(`${s.to}/`));
  const activeSub = START_SUBNAV.filter((s) => pathname === s.to || pathname.startsWith(`${s.to}/`)).sort((a, b) => b.to.length - a.to.length)[0]?.to;
  const strip = useRef<HTMLElement>(null);
  // A narrow strip scrolls: bring the current tab fully into view when the route changes.
  useEffect(() => {
    strip.current?.querySelector<HTMLElement>('[aria-current="page"]')?.scrollIntoView?.({ inline: "nearest", block: "nearest" });
  }, [activeSub]);
  return (
    <div className="aur-subband">
      <div className="aur-subtabs">
        {start && single && <div className="tabs sub"><span className="tab" aria-current="page">{single.label}</span></div>}
        {start && !single && (
          <nav className="tabs sub" aria-label="Start sections" ref={strip}>
            {START_SUBNAV.map((s) => <NavLink key={s.to} to={s.to} className="tab" data-testid={`subnav-${s.label.toLowerCase().replace(/\s+/g, "-")}`} aria-current={s.to === activeSub ? "page" : undefined}>{s.label}</NavLink>)}
          </nav>
        )}
        <div className="aur-subtabs-host" ref={setHost} />
      </div>
      <QuickBar />
    </div>
  );
}

/** [+ NEW CHARACTER | SAVE] | LEVEL UP | SHEET + attacks / spell-card icons | menu (10px SemiBold uppercase). Character-only items are disabled, not hidden. */
export function QuickBar() {
  const nav = useNavigate();
  const { snapshot, run } = useShellActions();
  const { setLevelUpOpen } = useShell();
  const off = !snapshot;
  return (
    <div className="aur-quickbar" role="toolbar" aria-label="Quick actions">
      {snapshot ? (
        <button type="button" onClick={() => run("save")} aria-label={snapshot.dirty ? "Save (unsaved changes)" : "Save"}>
          Save
        </button>
      ) : (
        <button type="button" onClick={() => run("new")}><Icon name="plus" />New character</button>
      )}
      <i className="sep" />
      <button type="button" disabled={off} className={snapshot?.level.levelUpAvailable ? "gold" : ""} onClick={() => setLevelUpOpen(true)}>Level Up</button><i className="sep" />
      <button type="button" disabled={off} onClick={() => nav("/sheet")}>Sheet</button>
      <ToolTip text="Attacks and spellcasting"><button type="button" disabled={off} aria-label="Attacks and spellcasting" onClick={() => nav("/manage")}><Icon name="swords" /></button></ToolTip>
      <ToolTip text="Spells"><button type="button" disabled={off} aria-label="Spells" onClick={() => nav("/spells")}><Icon name="cards" /></button></ToolTip>
      <i className="sep" />
      <ToolTip text="All screens"><button type="button" aria-label="All screens" onClick={() => nav("/more")}><Icon name="menu" /></button></ToolTip>
    </div>
  );
}

const STARTED_AT = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });

/** How long a plain status message stays on a phone before the line folds away (B-48 O7). */
export const STATUS_FOLD_MS = 4000;

/** O7: true once a plain message has been shown for 4 s. A pending choice or a running task keeps the line open. */
function useStatusFold(text: string, keepOpen: boolean, enabled: boolean): boolean {
  const [folded, setFolded] = useState(false);
  useEffect(() => {
    setFolded(false);
    if (!enabled || keepOpen) return;
    const t = setTimeout(() => setFolded(true), STATUS_FOLD_MS);
    return () => clearTimeout(t);
  }, [text, keepOpen, enabled]);
  return enabled && !keepOpen && folded;
}

/** Bottom status bar (26px). `compact` (phones) drops the file label and version. */
export function StatusBar({ compact, busy }: { compact?: boolean; busy?: boolean }) {
  const { snapshot, flow, setFlow, hasNext, goNext } = useShellActions();
  const { savedByUser } = useCharacter();
  const message = useStatusMessage();
  const phone = usePhone();
  const text = message || (snapshot ? `Loaded '${snapshot.name}'` : `Last checked for content updates at ${STARTED_AT}`);
  const folded = useStatusFold(text, hasNext || !!busy, phone);
  return (
    <footer className="aur-status" aria-label="Status" data-collapsed={phone ? String(folded) : undefined} data-pending={phone ? String(hasNext) : undefined}>
      <span className="status-msg" role="status">{text}</span>
      {busy && <MetroProgressBar label="Working" width={150} />}
      <span className="spacer" />
      {!compact && (
        <>
          <span className="btn-like file"><Icon name="file" />{(savedByUser && snapshot?.displayPath && displayPath(snapshot.displayPath)) || "No character file loaded"}</span>
          <i className="sep" />
        </>
      )}
      {hasNext && <button type="button" className="next" onClick={goNext}><Icon name="next-selection" />Next selection</button>}
      {hasNext && !compact && <i className="sep" />}
      {!compact && <button type="button" className={flow ? "flow-on" : "flow-off"} aria-pressed={flow} onClick={() => setFlow(!flow)}><Icon name="selection-flow" />Selection flow</button>}
      {!compact && <i className="sep" />}
      {!compact && <span className="version">v{__APP_VERSION__}</span>}
    </footer>
  );
}

/** Desktop (not mobile) Tauri webview: the OS title bar is replaced by the custom one. */
export function isDesktopTauri(): boolean {
  if (!isTauri()) return false;
  return !/Android|iPhone|iPad|iPod/i.test(navigator.userAgent) && !(navigator.maxTouchPoints > 1 && /Mac/.test(navigator.userAgent));
}

/** macOS keeps its native traffic lights (titleBarStyle Overlay), so the custom buttons are hidden there. */
export function hasNativeWindowButtons(): boolean {
  return isDesktopTauri() && /Mac/.test(navigator.userAgent);
}

type WindowAction = "minimize" | "toggleMaximize" | "close";

async function runWindowAction(action: WindowAction): Promise<void> {
  if (!isDesktopTauri()) return; // browser / MockApi: render only, never act
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow()[action]();
  } catch (e) {
    setStatus(`Window command failed: ${e instanceof Error ? e.message : String(e)}`);
  }
}

const GLYPH = { fill: "none", stroke: "currentColor", strokeWidth: 1, "aria-hidden": true } as const;

/** Minimize / maximize / close (34x30 flat buttons, glyph #E6E6E6, close hover red). */
export function WindowButtons() {
  return (
    <div className="aur-winbtns">
      <button type="button" aria-label="Minimize" onClick={() => void runWindowAction("minimize")}><svg width="10" height="10" viewBox="0 0 10 10" {...GLYPH}><path d="M0 9.5h10" /></svg></button>
      <button type="button" aria-label="Maximize" onClick={() => void runWindowAction("toggleMaximize")}><svg width="10" height="10" viewBox="0 0 10 10" {...GLYPH}><rect x="0.5" y="0.5" width="9" height="9" /></svg></button>
      <button type="button" className="close" aria-label="Close window" onClick={() => void runWindowAction("close")}><svg width="10" height="10" viewBox="0 0 10 10" {...GLYPH}><path d="M0 0l10 10M10 0L0 10" /></svg></button>
    </div>
  );
}
