import { useId, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { AiProviderId, AppSettings, Density } from "../../api/contract";
import { Alert, Button, ComboBox } from "../../components";
import { useApi } from "../../state/ApiContext";
import { usePlatform } from "../../state/platform";
import { useSettings } from "../../state/SettingsContext";
import { errorMessage, useAsync } from "../../state/useAsync";
import { UnverifiedBadge } from "../content/IndexesTab";
import { AiHostPanel } from "./AiHostPanel";
import { checkAppUpdateNow, openDownload, useAppUpdateView } from "../../state/appUpdate";
import { DescCheck, FieldLabel, Help, NumberField, SectionDesc } from "./rows";
import { SettingsWindow } from "./SettingsWindow";

export const BOREALIS_TABS = [
  { id: "ai", label: "AI assistant" }, { id: "phones", label: "Phones" }, { id: "content", label: "Content" }, { id: "general", label: "General" },
] as const;

type Tab = (typeof BOREALIS_TABS)[number]["id"];

/** Mobile has no AI at all; on desktop the AI tab holds the switch and "Phones" appears only once AI is on. */
function visibleTabs(mobile: boolean, aiEnabled: boolean) {
  return BOREALIS_TABS.filter((t) => (t.id === "ai" ? !mobile : t.id === "phones" ? aiEnabled : true));
}

/**
 * Borealis Settings: what the classic builder has no equivalent for. Same window chrome and rows as Application Settings,
 * but each change applies at once (the footer only closes the window).
 */
export function BorealisSettings({ onClose, initialTab }: { onClose: () => void; initialTab?: Tab }) {
  const api = useApi();
  const nav = useNavigate();
  const { settings: s, update, replace } = useSettings();
  const { mobile, aiEnabled } = usePlatform();
  const tabs = visibleTabs(mobile, aiEnabled);
  const [picked, setTab] = useState<Tab | undefined>(initialTab);
  const tab: Tab = tabs.some((t) => t.id === picked) ? picked! : tabs[0]!.id;
  const [aiError, setAiError] = useState<string | null>(null);
  const provider = useId();
  const ruleset = useId();
  const density = useId();
  const indexes = useAsync(() => api.listIndexes(), [api]);
  const [checking, setChecking] = useState(false);
  const { status: updateStatus } = useAppUpdateView();
  const checkNow = async () => { setChecking(true); try { await checkAppUpdateNow(api); } finally { setChecking(false); } };
  const set = (patch: Partial<AppSettings>) => void update(patch);
  const setAi = (patch: Partial<AppSettings["ai"]>) => set({ ai: { ...s.ai, ...patch } });

  /** The AI program is chosen in the backend's native dialog; the backend validates it and asks for confirmation. */
  const browseAiProgram = async () => {
    setAiError(null);
    try {
      const [pick] = await api.pickPath({ purpose: "ai-override" });
      if (pick) replace(await api.setAiOverride({ token: pick.token }));
    } catch (e) { setAiError(errorMessage(e)); }
  };
  const go = (to: string) => { onClose(); nav(to); };

  return (
    <SettingsWindow title="Borealis Settings" tabs={tabs} active={tab} onTab={(t) => setTab(t as Tab)} tabsLabel="Borealis settings sections" idPrefix="bset"
      onClose={onClose} narrowPad={tab !== "general"}
      footer={<><span className="spacer" /><Button className="aur-set-save" onClick={onClose}>Close</Button></>}>
      {tab === "ai" && (
        <>
          <h3 className="aur-set-title13 first flush">AI ASSISTANT</h3>
          <SectionDesc>The command line program that answers rules questions and drafts content.</SectionDesc>
          <DescCheck title="Enable AI assistant (optional)" description="Uses your own Claude or Codex command-line tool."
            checked={aiEnabled} onChange={(v) => setAi({ enabled: v })} />
          {aiEnabled && <>
          <FieldLabel htmlFor={provider}>AI PROVIDER</FieldLabel>
          <ComboBox id={provider} value={s.ai.provider} onChange={(e) => setAi({ provider: e.target.value as AiProviderId })}
            options={[{ value: "claude", label: "Claude CLI" }, { value: "codex", label: "Codex CLI" }, { value: "none", label: "Whichever is installed" }]} />
          <FieldLabel>AI PROGRAM</FieldLabel>
          <Help>{s.ai.overridePath ?? "Found automatically. Browse to use a different claude or codex program."}</Help>
          <div className="aur-set-buttons">
            <Button aria-label="Browse…" onClick={() => void browseAiProgram()}>Browse...</Button>
            {s.ai.overridePath && <Button onClick={() => setAi({ overridePath: null })}>Clear</Button>}
          </div>
          {aiError && <Alert tone="danger" role="alert">{aiError}</Alert>}
          <FieldLabel>AI TOOLS</FieldLabel>
          <Help>Check, install or sign in to the Claude and Codex tools.</Help>
          <div className="aur-set-buttons"><Button aria-label="AI setup…" onClick={() => go("/setup/ai")}>AI setup...</Button></div>
          </>}
        </>
      )}
      {tab === "phones" && aiEnabled && (
        <>
          <h3 className="aur-set-title13 first flush">PHONES AND TABLETS</h3>
          <SectionDesc>Phones and tablets cannot run the AI programs; they connect to a desktop instance.</SectionDesc>
          <AiHostPanel />
        </>
      )}
      {tab === "content" && (
        <>
          <h3 className="aur-set-title13 first flush">CONTENT</h3>
          <SectionDesc>Registered content indexes and the ruleset new characters start with.</SectionDesc>
          <FieldLabel htmlFor={ruleset}>DEFAULT RULESET FOR NEW CHARACTERS</FieldLabel>
          <ComboBox id={ruleset} value={s.defaultRuleset} onChange={(e) => set({ defaultRuleset: e.target.value as AppSettings["defaultRuleset"] })}
            options={[{ value: "dnd2024", label: "2024 (SRD 5.2)" }, { value: "dnd2014", label: "2014 (SRD 5.1)" }]} />
          <FieldLabel>CONTENT INDEXES</FieldLabel>
          <Help>{indexes.data && indexes.data.length === 0 ? "None registered." : "Indexes you added under Additional Content."}</Help>
          {(indexes.data ?? []).map((i) => (
            <div key={i.url} className="aur-set-index"><strong>{i.name}{!i.verified && <UnverifiedBadge />}</strong><Help>{i.url}</Help></div>
          ))}
          <div className="aur-set-buttons"><Button aria-label="Manage…" onClick={() => go("/content")}>Manage...</Button></div>
        </>
      )}
      {tab === "general" && (
        <>
          <h3 className="aur-set-title13 first flush">GENERAL</h3>
          <SectionDesc>Saving, display and rules limits that only Borealis has.</SectionDesc>
          <DescCheck title="Autosave" description="Save changes automatically after a short pause." checked={s.autosave} onChange={(v) => set({ autosave: v })} />
          <DescCheck title="Generate the sheet preview automatically" checked={s.sheet.autoGenerate} onChange={(v) => set({ sheet: { ...s.sheet, autoGenerate: v } })} />
          <DescCheck title="Open the sheet after saving" checked={s.sheet.openAfterSave} onChange={(v) => set({ sheet: { ...s.sheet, openAfterSave: v } })} />
          <DescCheck title="High contrast" description="Stronger borders, text and focus rings." checked={s.highContrast} onChange={(v) => set({ highContrast: v })} />
          <FieldLabel htmlFor={density}>DENSITY</FieldLabel>
          <ComboBox id={density} value={s.density} onChange={(e) => set({ density: e.target.value as Density })}
            options={[{ value: "comfortable", label: "Comfortable" }, { value: "compact", label: "Compact" }]} />
          <NumberField label="POINT-BUY BUDGET" value={s.pointBuyBudget} min={1} max={60} onChange={(n) => set({ pointBuyBudget: n })} />
          <NumberField label="MINIMUM BASE SCORE" value={s.abilityMinBase} min={1} max={20} onChange={(n) => set({ abilityMinBase: n })} />
          <NumberField label="MAXIMUM BASE SCORE" value={s.abilityMaxBase} min={8} max={30} onChange={(n) => set({ abilityMaxBase: n })} />
          <FieldLabel>UPDATES</FieldLabel>
          <DescCheck title="Check for new versions of Borealis" description="Only asks GitHub for the latest version number; nothing about you or your characters is sent."
            checked={s.checkAppUpdates} onChange={(v) => set({ checkAppUpdates: v })} />
          <div className="aur-set-buttons">
            <Button disabled={checking} onClick={() => void checkNow()}>Check now</Button>
            {updateStatus?.state === "done" && updateStatus.newer && <Button onClick={() => void openDownload(api, updateStatus.url ?? undefined)}>Download</Button>}
          </div>
          <p className="aur-set-help" role="status">{updateText(updateStatus)}</p>
        </>
      )}
    </SettingsWindow>
  );
}

/** The line under "Check now": the last answer in words. */
export function updateText(u: ReturnType<typeof useAppUpdateView>["status"]): string {
  if (!u || u.state === "idle" || u.state === "disabled") return "";
  if (u.state === "failed") return "Could not reach GitHub. Check your connection and try again.";
  return u.newer ? `Borealis ${u.latest} is available (you have ${u.current}).` : `You have the latest version (${u.current}).`;
}
