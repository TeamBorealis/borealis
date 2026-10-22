import { NavLink, useLocation } from "react-router";
import { Icon } from "../Icon";
import { BrandMark } from "./BrandMark";
import { MAIN_TABS, activeTab } from "./sections";
import { useShell } from "./ShellContext";
import { useShellActions } from "./useShellActions";
import { UpdateCommand } from "./UpdateFlyout";

/** Phone title bar: hamburger, character name (gold) or app name, the update button when a release is waiting, settings. Honours the top safe-area inset. */
export function MobileTitleBar({ onMenu }: { onMenu: () => void }) {
  const { snapshot } = useShellActions();
  const { openSettings } = useShell();
  return (
    <header className="aur-title mobile">
      <button type="button" className="aur-iconbtn" aria-label="Menu" onClick={onMenu}><Icon name="menu" /></button>
      <BrandMark />
      <div className={`aur-title-text ${snapshot ? "gold" : ""}`}>{snapshot ? snapshot.name : "Borealis"}</div>
      <UpdateCommand compact />
      <button type="button" className="aur-iconbtn" aria-label="Settings" onClick={() => openSettings()}><Icon name="cog" /></button>
    </header>
  );
}

/** Phone bottom bar, same language as the main tab strip: accent bar, 2px highlight line, pointer on the selected item. */
export function BottomBar() {
  const { pathname } = useLocation();
  const { snapshot } = useShellActions();
  const active = activeTab(pathname);
  const primary = MAIN_TABS.filter((t) => !t.secondary);
  return (
    <nav className="aur-bottombar" aria-label="Main">
      {primary.map((t) => {
        const off = t.needsCharacter && !snapshot;
        const remaining = t.id === "build" && snapshot ? snapshot.progress.remaining : 0;
        const body = (<><Icon name={t.icon} /><span className="label">{t.short}</span>{remaining > 0 && <span className="badge-count" aria-label={`${remaining} choices remaining`}>{remaining}</span>}</>);
        return off
          ? <span key={t.id} className="bb-item disabled" aria-disabled="true">{body}</span>
          : <NavLink key={t.id} to={t.to} role="link" aria-label={remaining > 0 ? `${t.short}, ${remaining} choices remaining` : t.short} data-testid={`nav-${t.id}`} className="bb-item" aria-current={t.id === active.id ? "page" : undefined} data-selected={t.id === active.id}>{body}</NavLink>;
      })}
      <NavLink to="/more" role="link" aria-label="More" data-testid="nav-more" className="bb-item" data-selected={pathname === "/more" || (!primary.includes(active) && active.id !== "start")}><Icon name="more" /><span className="label">More</span></NavLink>
    </nav>
  );
}
