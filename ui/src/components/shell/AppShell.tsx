import { type ReactNode, useEffect, useState } from "react";
import { Flyout } from "../controls/Flyout";
import { OverlayHostContext } from "../controls/OverlayHost";
import { BrandMark } from "./BrandMark";
import { MainTabs, ProfileBlock, SearchBox, StatusBar, SubBand, TitleCommands, WindowButtons, hasNativeWindowButtons, isDesktopTauri } from "./Chrome";
import { MenuBar, MenuTree } from "./MenuBar";
import { BottomBar, MobileTitleBar } from "./MobileChrome";
import { ShellProvider, useShell } from "./ShellContext";
import { LevelPanel } from "../../pages/builder/LevelPanel";
import { useNarrow } from "./useNarrow";
import { useContentUpdateNotice } from "./useContentUpdateNotice";
import { useAppUpdateWatcher } from "../../state/appUpdate";
import { UpdateFlyout } from "./UpdateFlyout";
import { useShellActions } from "./useShellActions";
import { useCharacter } from "../../state/CharacterContext";
import { OptionsFlyout } from "../../pages/manage/OptionsFlyout";
import { SaveFlyout } from "./SaveFlyout";
import { BundleHost } from "../../pages/transfer/BundleHost";
import { TransferHost } from "../../pages/transfer/TransferHost";
import { EditionFlyout } from "../../pages/manage/EditionFlyout";
import { SettingsHost } from "../../pages/settings/SettingsHost";
import { CompendiumFlyout } from "../../pages/compendium/CompendiumFlyout";

function useShortcuts() {
  const { run, snapshot } = useShellActions();
  const { setPortraitsOpen, compendium, closeCompendium } = useShell();
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k === "s") { e.preventDefault(); run("save"); }
      else if (k === "n" && !e.shiftKey) { e.preventDefault(); run("new"); }
      else if (k === "p" && !e.shiftKey && snapshot) { e.preventDefault(); setPortraitsOpen(true); }
    };
    const fkey = (e: KeyboardEvent) => { if (e.key === "F1") { e.preventDefault(); if (compendium) closeCompendium(); else run("compendium"); } else if (e.key === "F8") { e.preventDefault(); run("options"); } };
    window.addEventListener("keydown", fkey);
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("keydown", key); window.removeEventListener("keydown", fkey); };
  }, [run, snapshot, setPortraitsOpen, compendium, closeCompendium]);
}

/** Keyboard shortcuts live inside the provider so they can open shell flyouts. */
function Shortcuts() {
  useShortcuts();
  useContentUpdateNotice();
  useAppUpdateWatcher();
  return null;
}

/**
 * classic-builder-style window: title bar + menu (32px), profile/main tab strip (60px), sub-tab band (30px), content, status bar (26px).
 * Below 900px: hamburger title bar, scrolling sub-tab band, 26px status line and a bottom tab bar.
 */
function LevelUpFlyout() {
  const { levelUpOpen, setLevelUpOpen } = useShell();
  const { snapshot } = useCharacter();
  if (!levelUpOpen || !snapshot) return null;
  return <Flyout title="Level up & multiclassing" onClose={() => setLevelUpOpen(false)}><LevelPanel embedded /></Flyout>;
}

export function AppShell({ children }: { children: ReactNode }) {
  const narrow = useNarrow();
  const [menuOpen, setMenuOpen] = useState(false);
  // Modal overlays portal into the window root, outside the scrolling <main> (see OverlayHost).
  const [overlayHost, setOverlayHost] = useState<HTMLDivElement | null>(null);
  const { busy } = useCharacter();
  const native = hasNativeWindowButtons();
  return (
    <ShellProvider>
      <Shortcuts />
      <OverlayHostContext.Provider value={overlayHost}>
        <div ref={setOverlayHost} className={`aur-window ${narrow ? "narrow" : "wide"}`}>
          <a className="skip-link" href="#main">Skip to content</a>
          {narrow && isDesktopTauri() && !native && (
            <div className="aur-winstrip" data-tauri-drag-region><WindowButtons /></div>
          )}
          {narrow ? <MobileTitleBar onMenu={() => setMenuOpen(true)} /> : (
            <header className={`aur-title ${native ? "native-controls" : ""}`} data-tauri-drag-region>
              <BrandMark className="aur-logo" />
              <MenuBar />
              <div className="aur-title-text">Borealis</div>
              <SearchBox />
              <TitleCommands />
              {!native && <WindowButtons />}
            </header>
          )}
          {!narrow && (
            <div className="aur-chrome">
              <MainTabs />
              <ProfileBlock />
            </div>
          )}
          <SubBand />
          <main id="main" className="aur-content" tabIndex={-1}>{children}</main>
          <StatusBar compact={narrow} busy={busy} />
          {narrow && <BottomBar />}
          <LevelUpFlyout />
          <CompendiumFlyout />
          <OptionsFlyout />
          <EditionFlyout />
          <SaveFlyout />
          <UpdateFlyout />
          <SettingsHost />
          <TransferHost />
          <BundleHost />
          {narrow && menuOpen && (
            <Flyout title="Menu" position="left" width="settings" onClose={() => setMenuOpen(false)}>
              <MenuTree onDone={() => setMenuOpen(false)} />
            </Flyout>
          )}
        </div>
      </OverlayHostContext.Provider>
    </ShellProvider>
  );
}
