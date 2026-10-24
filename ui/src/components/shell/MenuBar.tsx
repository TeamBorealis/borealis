import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Icon } from "../Icon";
import { usePlatform } from "../../state/platform";
import { CHARACTER_MENU, type MenuEntry, openProjectSite, visibleMenus } from "./sections";
import { useShell } from "./ShellContext";
import { openEditionFlyout } from "../../pages/manage/EditionFlyout";
import { useShellActions } from "./useShellActions";
import { openExportBundle, openReceiveFromDevice, openSendToDevice } from "../../pages/transfer/openTransfer";
import { pickAndOpenBundle } from "../../pages/transfer/openBundle";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";

interface MenuModel { id: string; label: ReactNode; entries: readonly MenuEntry[]; gold?: boolean }

/** Entries a menu can show in the current state. */
function visibleEntries(entries: readonly MenuEntry[], hasCharacter: boolean): MenuEntry[] {
  const list = [...entries];
  return list.filter((e, i) => !(e.separator && (i === 0 || i === list.length - 1 || list[i - 1]?.separator))).map((e) => ({ ...e, needsCharacter: e.needsCharacter && !hasCharacter }));
}

/** FILE / VIEW / HELP + the loaded character's name (gold). Desktop dropdown menus with roving keyboard support. */
export function MenuBar() {
  const { snapshot } = useShellActions();
  const { aiEnabled } = usePlatform();
  const [open, setOpen] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const menus: MenuModel[] = [
    ...visibleMenus(aiEnabled).map((m) => ({ id: m.id, label: m.label, entries: m.entries })),
    ...(snapshot ? [{ id: "character", label: snapshot.name, entries: CHARACTER_MENU, gold: true }] : []),
  ];
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) setOpen(null); };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") { setOpen(null); return; }
    const i = menus.findIndex((m) => m.id === open);
    if (open && (e.key === "ArrowRight" || e.key === "ArrowLeft") && (e.target as HTMLElement).closest("[role=menubar]")) {
      e.preventDefault();
      const n = menus[(i + (e.key === "ArrowRight" ? 1 : -1) + menus.length) % menus.length]!;
      setOpen(n.id);
      ref.current?.querySelector<HTMLElement>(`#menu-${n.id}`)?.focus();
    }
  };
  return (
    <div className="aur-menubar" role="menubar" aria-label="Application menu" ref={ref} onKeyDown={onKey}>
      {menus.map((m) => (
        <div key={m.id} className="aur-menu">
          <button type="button" id={`menu-${m.id}`} role="menuitem" aria-haspopup="menu" aria-expanded={open === m.id} className={`aur-menu-top ${m.gold ? "gold" : ""}`}
            onClick={() => setOpen(open === m.id ? null : m.id)} onMouseEnter={() => open && setOpen(m.id)}>
            {m.label}
          </button>
          {open === m.id && <MenuPanel labelledBy={`menu-${m.id}`} entries={m.entries} onDone={() => setOpen(null)} />}
        </div>
      ))}
    </div>
  );
}

/** Runs a menu entry: route, project-site link, flyout, or shell action. */
function useActivate() {
  const nav = useNavigate();
  const api = useApi();
  const { run } = useShellActions();
  const { setLevelUpOpen, setPortraitsOpen, openSettings, setBorealisSettingsOpen } = useShell();
  return (e: MenuEntry) => {
    if (e.to) nav(e.to);
    else if (e.site !== undefined) openProjectSite(e.site);
    else if (e.action === "levelup") setLevelUpOpen(true);
    else if (e.action === "portraits") setPortraitsOpen(true);
    else if (e.action === "settings") openSettings();
    else if (e.action === "borealis-settings") setBorealisSettingsOpen(true);
    else if (e.action === "edition") openEditionFlyout();
    else if (e.action === "send-to-device") openSendToDevice();
    else if (e.action === "receive-from-device") openReceiveFromDevice();
    else if (e.action === "export-bundle") openExportBundle();
    else if (e.action === "open-bundle") void pickAndOpenBundle(api).catch((err) => setStatus(errorMessage(err)));
    else if (e.action) run(e.action);
  };
}

function MenuPanel({ entries, labelledBy, onDone }: { entries: readonly MenuEntry[]; labelledBy: string; onDone: () => void }) {
  const activate = useActivate();
  const { flow, snapshot } = useShellActions();
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => first.current?.focus(), []);
  const items = visibleEntries(entries, !!snapshot);
  const act = (e: MenuEntry) => {
    onDone();
    activate(e);
  };
  const move = (ev: KeyboardEvent) => {
    const dir = ev.key === "ArrowDown" ? 1 : ev.key === "ArrowUp" ? -1 : 0;
    if (!dir) return;
    ev.preventDefault();
    const els = [...(ev.currentTarget as HTMLElement).querySelectorAll<HTMLElement>("button:not([disabled])")];
    const at = els.indexOf(document.activeElement as HTMLElement);
    els[(at + dir + els.length) % els.length]?.focus();
  };
  let firstSet = false;
  return (
    <div className="aur-menu-panel" role="menu" aria-labelledby={labelledBy} onKeyDown={move}>
      {items.map((e) => {
        if (e.separator) return <div key={e.id} role="separator" className="aur-menu-sep" />;
        const ref = !firstSet && !e.needsCharacter ? first : undefined;
        if (ref) firstSet = true;
        return (
          <button key={e.id} ref={ref} type="button" role={e.checkable ? "menuitemcheckbox" : "menuitem"} aria-checked={e.checkable ? flow : undefined}
            disabled={e.needsCharacter} className="aur-menu-item" onClick={() => act(e)}>
            <span className="aur-menu-icon">{e.checkable && flow ? <Icon name="check" /> : null}</span>
            <span className="aur-menu-label">{e.label}</span>
            {e.shortcut && <span className="aur-menu-gesture">{e.shortcut}</span>}
          </button>
        );
      })}
    </div>
  );
}

/** Phone hamburger content: the same menus as an accordion list. */
export function MenuTree({ onDone }: { onDone: () => void }) {
  const activate = useActivate();
  const { flow, snapshot } = useShellActions();
  const { aiEnabled } = usePlatform();
  const groups: MenuModel[] = [
    ...visibleMenus(aiEnabled).map((m) => ({ id: m.id, label: m.label, entries: m.entries })),
    ...(snapshot ? [{ id: "character", label: snapshot.name, entries: CHARACTER_MENU, gold: true }] : []),
  ];
  return (
    <nav className="menu-tree" aria-label="Menu">
      {groups.map((g) => (
        <div key={g.id} className="menu-tree-group">
          <div className={`menu-tree-head ${g.gold ? "gold" : ""}`}>{g.label}</div>
          {visibleEntries(g.entries, !!snapshot).filter((e) => !e.separator).map((e) => (
            <button key={e.id} type="button" className="menu-tree-item" disabled={e.needsCharacter} aria-pressed={e.checkable ? flow : undefined}
              onClick={() => { onDone(); activate(e); }}>
              <span className="aur-menu-icon">{e.checkable && flow ? <Icon name="check" /> : null}</span>{e.label}
            </button>
          ))}
        </div>
      ))}
    </nav>
  );
}
