import { useCallback, useEffect, useRef, useState } from "react";
import type { AppSettings, DataDirs } from "../../api/contract";
import { Alert, Button } from "../../components";
import { applyTheme, useSettings } from "../../state/SettingsContext";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage, useAsync } from "../../state/useAsync";
import { AppearanceTab } from "./AppearanceTab";
import { DataFilesTab, type HeldPicks } from "./DataFilesTab";
import { BuildTab, EquipmentTab, GeneralTab } from "./GeneralBuildTabs";
import { SectionDesc } from "./rows";
import { SettingsWindow } from "./SettingsWindow";
import { SheetTab } from "./SheetTab";

/** The classic builder's tab order (DEVELOPER omitted). */
export const SETTINGS_TABS = [
  { id: "general", label: "General" }, { id: "appearance", label: "Appearance" }, { id: "build", label: "Character build" },
  { id: "data", label: "Data files" }, { id: "equipment", label: "Equipment" }, { id: "sheet", label: "Character sheet" },
  { id: "license", label: "License" }, { id: "about", label: "About" },
] as const;

/** The fields this window owns; everything else (Borealis Settings) is taken from the stored settings on Save. */
const OWNED: (keyof AppSettings)[] = [
  "playerName", "abilityMethod", "rowHeight", "selectionFlow", "skillsPanelExpanded", "savingThrowsExpanded", "confirmRemoveLevel",
  "theme", "accentColor", "portraitSize", "autoContentUpdate", "showImportNotice", "abilityCapEnabled", "confirmEquipWeaponNotInAttacks",
];
const SHEET_OWNED: (keyof AppSettings["sheet"])[] = [
  "formFillable", "theme", "edition", "styleFormatting", "swapAbilityPositions", "includeNonPreparedSpells", "cards", "cardsNewPage", "legacySpellcastingPage",
];
const WIDE_TABS = new Set(["general", "build", "data"]);

/** Full form to send on Save: the form's own fields over the stored settings, with folders copied from the stored ones. */
function mergeForm(stored: AppSettings, form: AppSettings): AppSettings {
  const next: Record<string, unknown> = { ...stored };
  for (const k of OWNED) if (k in form) next[k] = form[k];
  const sheet: Record<string, unknown> = { ...stored.sheet };
  for (const k of SHEET_OWNED) sheet[k] = form.sheet[k];
  return { ...(next as unknown as AppSettings), sheet: sheet as unknown as AppSettings["sheet"] };
}

/**
 * Application Settings (classic builder). Every edit is held in `form`; accent and theme preview live and revert on Cancel.
 * Save writes folders (token flow) then the full settings; Restore Defaults saves at once and stays open.
 */
export function ApplicationSettings({ initialTab, onClose }: { initialTab: string; onClose: () => void }) {
  const api = useApi();
  const { settings: persisted, replace } = useSettings();
  const [tab, setTab] = useState(SETTINGS_TABS.some((t) => t.id === initialTab) ? initialTab : "general");
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

  const browse = async (which: "documents" | "secondary") => {
    setError(null);
    try {
      const current = which === "documents" ? held.documents?.path ?? dirs?.documentsDir : held.secondary && held.secondary !== "unlink" ? held.secondary.path : dirs?.secondaryContentDir ?? undefined;
      const [pick] = await api.pickPath({ purpose: which === "documents" ? "documents-dir" : "secondary-content-dir", startDir: current });
      if (pick) setHeld((h) => ({ ...h, [which]: { token: pick.token, path: pick.path } }));
    } catch (e) { setError(errorMessage(e)); }
  };

  const save = async () => {
    if (!form || saving) return;
    setSaving(true);
    setError(null);
    try {
      let skipped = 0;
      if (held.documents) await api.setDocumentsDir({ token: held.documents.token });
      if (held.secondary === "unlink") await api.unlinkSecondaryContentDir();
      else if (held.secondary) skipped = (await api.setSecondaryContentDir({ token: held.secondary.token })).report.skipped.length;
      const stored = await api.getSettings();
      const saved = await api.saveSettings({ patch: mergeForm(stored, form) });
      replace(saved);
      setStatus(skipped ? `Your preferences have been saved. ${skipped} secondary content file(s) could not be loaded.` : "Your preferences have been saved.");
      onClose();
    } catch (e) {
      setError(errorMessage(e));
      setSaving(false);
    }
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
      case "license": return (
        <>
          <h3 className="aur-set-title13 first flush">LICENSE</h3>
          <SectionDesc>Attribution and terms</SectionDesc>
          <p className="aur-set-text">Bundled sample content is released under CC-BY-4.0 and attributed in each content pack. Content you import stays on your device and is not redistributed.</p>
        </>
      );
      default: return (
        <div className="aur-set-about">
          <h3 className="aur-set-title13 first flush">BOREALIS</h3>
          <div className="aur-set-version">{version.data ? `v${version.data}` : ""}</div>
          <p className="aur-set-text">A character builder for fifth edition. Borealis collects no telemetry.</p>
        </div>
      );
    }
  })();

  return (
    <SettingsWindow title="Application Settings" tabs={SETTINGS_TABS} active={tab} onTab={setTab} tabsLabel="Settings sections" idPrefix="settings"
      onClose={onClose} onSubmit={() => void save()} narrowPad={!WIDE_TABS.has(tab)}
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
