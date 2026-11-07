import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import type { AppSettings, DataDirs } from "../../api/contract";
import { Alert, Button } from "../../components";
import { applyTheme, useSettings } from "../../state/SettingsContext";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { usePlatform } from "../../state/platform";
import { setStatus } from "../../state/status";
import { errorMessage, useAsync } from "../../state/useAsync";
import { openEditionFlyout } from "../manage/EditionFlyout";
import { AppearanceTab } from "./AppearanceTab";
import { AiTab, ContentTab, DisplayTab, PhonesTab, RulesTab, SavingTab, UpdatesTab } from "./BorealisTabs";
import { DataFilesTab, heldPath, type HeldFolder, type HeldPicks } from "./DataFilesTab";
import { DevicesTab } from "./DevicesTab";
import { BuildTab, EquipmentTab, GeneralTab } from "./GeneralBuildTabs";
import { ModelLicence } from "./ModelLicence";
import { SectionDesc } from "./rows";
import { SearchTab } from "./SearchTab";
import { formChanged, mergeForm } from "./settingsForm";
import { resolveSettingsTab, visibleSettingsTabs } from "./settingsTabs";
import { SettingsWindow } from "./SettingsWindow";
import { SheetTab } from "./SheetTab";
import { PhoneCase } from "../../lib/phoneCase";
import type { PickPurpose } from "../../api/contract";

/** The backend picker purpose behind each folder's Browse button. */
const PICK_PURPOSE: Record<HeldFolder, PickPurpose> = {
  documents: "documents-dir", secondary: "secondary-content-dir", portraits: "portrait-library-dir",
};

/**
 * Application Settings: the one settings window (GitHub #31). The classic builder's tabs first, then the Borealis-only tabs
 * (settingsTabs.ts). Every field edit is held in `form`; accent and theme preview live and revert on Cancel. Save writes
 * folders (token flow) then the full settings; Restore Defaults saves at once and stays open.
 */
export function ApplicationSettings({ initialTab, onClose }: { initialTab: string; onClose: () => void }) {
  const api = useApi();
  const nav = useNavigate();
  const { snapshot } = useCharacter();
  const { settings: persisted, replace } = useSettings();
  const { aiAvailable, aiEnabled, storeBuild, mobile } = usePlatform();
  const tabs = visibleSettingsTabs({ aiAvailable, aiEnabled, storeBuild });
  const [picked, setTab] = useState(initialTab);
  const tab = resolveSettingsTab(picked, tabs);
  const [form, setForm] = useState<AppSettings | null>(null);
  const [dirs, setDirs] = useState<DataDirs | null>(null);
  const [held, setHeld] = useState<HeldPicks>({});
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [cacheCleared, setCacheCleared] = useState(false);
  const version = useAsync(() => api.appVersion(), [api]);
  const latest = useRef(persisted);
  latest.current = persisted;

  useEffect(() => {
    let alive = true;
    Promise.all([api.getSettings(), api.getDataDirs()]).then(([s, d]) => { if (alive) { setForm(s); setDirs(d); } }, (e) => alive && setError(errorMessage(e)));
    return () => { alive = false; };
  }, [api]);
  // Accent and theme preview; closing without saving puts the stored look back.
  useEffect(() => { if (form) applyTheme(form); }, [form]);
  useEffect(() => () => applyTheme(latest.current), []);

  const set = useCallback((patch: Partial<AppSettings>) => setForm((f) => (f ? { ...f, ...patch } : f)), []);
  const setSheet = useCallback((sheet: AppSettings["sheet"]) => setForm((f) => (f ? { ...f, sheet } : f)), []);

  const browse = async (which: HeldFolder) => {
    setError(null);
    try {
      const stored = { documents: dirs?.documentsDir, secondary: dirs?.secondaryContentDir, portraits: dirs?.portraitLibraryDir }[which];
      const [pick] = await api.pickPath({ purpose: PICK_PURPOSE[which], startDir: heldPath(held[which]) ?? stored ?? undefined });
      if (pick) setHeld((h) => ({ ...h, [which]: { token: pick.token, path: pick.path } }));
    } catch (e) { setError(errorMessage(e)); }
  };

  /** Save, close, then run `after` (a button that leaves the window: AI setup, Manage, the Edition flyout). */
  const save = async (after?: () => void) => {
    if (!form || saving) return;
    setSaving(true);
    setError(null);
    try {
      let skipped = 0;
      if (held.documents) await api.setDocumentsDir({ token: held.documents.token });
      if (held.secondary === "unlink") await api.unlinkSecondaryContentDir();
      else if (held.secondary) skipped = (await api.setSecondaryContentDir({ token: held.secondary.token })).report.skipped.length;
      if (held.portraits) await api.setPortraitLibraryDir(held.portraits === "clear" ? {} : { token: held.portraits.token });
      const stored = await api.getSettings();
      const saved = await api.saveSettings({ patch: mergeForm(stored, form) });
      replace(saved);
      setStatus(skipped ? `Your preferences have been saved. ${skipped} secondary content file(s) could not be loaded.` : "Your preferences have been saved.");
      onClose();
      after?.();
    } catch (e) {
      setError(errorMessage(e));
      setSaving(false);
    }
  };

  /** Leave the window for another screen: unsaved edits are saved first (nothing to save = just close). */
  const leave = (after: () => void) => {
    if (form && formChanged(persisted, form, !!held.documents || !!held.secondary || !!held.portraits)) { void save(after); return; }
    onClose();
    after();
  };

  const restore = async () => {
    setError(null);
    try {
      const defaults = await api.restoreDefaultSettings();
      replace(defaults);
      setForm(defaults);
      setHeld({});
      setCacheCleared(false);
      setDirs(await api.getDataDirs());
    } catch (e) { setError(errorMessage(e)); }
  };

  const clearCache = async () => {
    try { await api.clearCaches(); setCacheCleared(true); } catch (e) { setError(errorMessage(e)); }
  };

  const body = !form ? null : (() => {
    switch (tab) {
      case "general": return <GeneralTab form={form} set={set} />;
      case "appearance": return <AppearanceTab form={form} set={set} />;
      case "build": return <BuildTab form={form} set={set} />;
      case "data": return <DataFilesTab form={form} set={set} dirs={dirs} held={held} browse={(w) => void browse(w)} unlink={() => setHeld((h) => ({ ...h, secondary: "unlink" }))} clearCache={() => void clearCache()} cacheCleared={cacheCleared} error={error} />;
      case "equipment": return <EquipmentTab form={form} set={set} />;
      case "sheet": return <SheetTab sheet={form.sheet} onChange={setSheet} />;
      case "rules": return <RulesTab form={form} set={set} hasCharacter={!!snapshot} openHouseRules={() => leave(openEditionFlyout)} />;
      case "saving": return <SavingTab form={form} set={set} />;
      case "display": return <DisplayTab form={form} set={set} />;
      case "content": return (
        <ContentTab manage={() => leave(() => nav("/content"))}
          portraits={mobile ? null : { dirs, held: held.portraits, browse: () => void browse("portraits"), clear: () => setHeld((h) => ({ ...h, portraits: "clear" })) }} />
      );
      case "search": return <SearchTab />;
      case "ai": return <AiTab form={form} set={set} setup={() => leave(() => nav("/setup/ai"))} />;
      case "phones": return <PhonesTab />;
      case "devices": return <DevicesTab />;
      case "updates": return <UpdatesTab form={form} set={set} />;
      case "license": return (
        <>
          <h3 className="aur-set-title13 first flush"><PhoneCase>LICENSE</PhoneCase></h3>
          <SectionDesc>Attribution and terms</SectionDesc>
          <p className="aur-set-text">Bundled sample content is released under CC-BY-4.0 and attributed in each content pack. Content you import stays on your device and is not redistributed.</p>
        </>
      );
      default: return (
        <div className="aur-set-about">
          <h3 className="aur-set-title13 first flush"><PhoneCase>BOREALIS</PhoneCase></h3>
          <div className="aur-set-version">{version.data ? `v${version.data}` : ""}</div>
          <p className="aur-set-text">A character builder for fifth edition. Borealis collects no telemetry. Import bug reports are only sent when you choose to send one.</p>
          <ModelLicence />
        </div>
      );
    }
  })();

  return (
    <SettingsWindow title="Application Settings" tabs={tabs} active={tab} onTab={setTab} tabsLabel="Settings sections" idPrefix="settings"
      onClose={onClose} onSubmit={() => void save()} narrowPad={!tabs.find((t) => t.id === tab)?.wide}
      footer={
        <>
          <Button icon="restore" className="aur-set-restore" disabled={!form} onClick={() => void restore()}>Restore defaults</Button>
          <span className="spacer" />
          <Button variant="success" type="submit" className="aur-set-save" disabled={!form || saving}>Save</Button>
          <Button variant="clear" className="aur-set-cancel" onClick={onClose}>Cancel</Button>
        </>
      }>
      {body}
      {tab !== "data" && error && <Alert tone="danger" role="alert">{error}</Alert>}
    </SettingsWindow>
  );
}
