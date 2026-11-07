import { useId, useState } from "react";
import type { AiProviderId, AppSettings, Density } from "../../api/contract";
import { Alert, Button, ComboBox } from "../../components";
import { useApi } from "../../state/ApiContext";
import { checkAppUpdateNow, openDownload, useAppUpdateView } from "../../state/appUpdate";
import { useSettings } from "../../state/SettingsContext";
import { errorMessage, useAsync } from "../../state/useAsync";
import { PhoneCase } from "../../lib/phoneCase";
import { UnverifiedBadge } from "../content/IndexesTab";
import { AiHostPanel } from "./AiHostPanel";
import { DescCheck, FieldLabel, Help, NumberField, SectionDesc, SectionTitle } from "./rows";

/**
 * The Borealis-only tabs of the settings window (GitHub #31): the same rows as the classic builder's tabs. Plain fields edit
 * the form and wait for Save; buttons that run something (browse, check, pair) act at once, like the classic builder's
 * cache button.
 */
interface FormProps { form: AppSettings; set: (patch: Partial<AppSettings>) => void }

function TabTitle({ title, desc }: { title: string; desc: string }) {
  return (
    <>
      <h3 className="aur-set-title13 first flush"><PhoneCase>{title}</PhoneCase></h3>
      <SectionDesc>{desc}</SectionDesc>
    </>
  );
}

/** RULES: edition and rules limits for every character; per-character house rules live in the character's Edition flyout. */
export function RulesTab({ form, set, hasCharacter, openHouseRules }: FormProps & { hasCharacter: boolean; openHouseRules: () => void }) {
  const ruleset = useId();
  return (
    <>
      <TabTitle title="RULES" desc="The edition new characters start with and the limits of ability score generation." />
      <FieldLabel htmlFor={ruleset}>DEFAULT RULESET FOR NEW CHARACTERS</FieldLabel>
      <ComboBox id={ruleset} value={form.defaultRuleset} onChange={(e) => set({ defaultRuleset: e.target.value as AppSettings["defaultRuleset"] })}
        options={[{ value: "dnd2024", label: "2024 (SRD 5.2)" }, { value: "dnd2014", label: "2014 (SRD 5.1)" }]} />
      <SectionTitle>ABILITY SCORES</SectionTitle>
      <NumberField label="POINT-BUY BUDGET" value={form.pointBuyBudget} min={1} max={60} onChange={(n) => set({ pointBuyBudget: n })} />
      <NumberField label="MINIMUM BASE SCORE" value={form.abilityMinBase} min={1} max={20} onChange={(n) => set({ abilityMinBase: n })} />
      <NumberField label="MAXIMUM BASE SCORE" value={form.abilityMaxBase} min={8} max={30} onChange={(n) => set({ abilityMaxBase: n })} />
      <SectionTitle>HOUSE RULES</SectionTitle>
      <SectionDesc>
        House rules belong to one character: unrestricted point buy, ignore feat prerequisites, ignore multiclass prerequisites
        and the attunement limit. Change them in the character's Edition &amp; compatibility.
      </SectionDesc>
      <div className="aur-set-buttons">
        <Button aria-label="Edition & compatibility…" disabled={!hasCharacter} onClick={openHouseRules}>Edition &amp; compatibility...</Button>
      </div>
      {!hasCharacter && <Help>Open a character to change its house rules.</Help>}
    </>
  );
}

/** SAVING: autosave and what happens to the sheet. */
export function SavingTab({ form, set }: FormProps) {
  const sheet = (patch: Partial<AppSettings["sheet"]>) => set({ sheet: { ...form.sheet, ...patch } });
  return (
    <>
      <TabTitle title="SAVING" desc="When your characters are saved and what happens to the sheet." />
      <DescCheck title="Autosave" description="Save changes automatically after a short pause." checked={form.autosave} onChange={(v) => set({ autosave: v })} />
      <DescCheck title="Generate the sheet preview automatically" checked={form.sheet.autoGenerate} onChange={(v) => sheet({ autoGenerate: v })} />
      <DescCheck title="Open the sheet after saving" checked={form.sheet.openAfterSave} onChange={(v) => sheet({ openAfterSave: v })} />
    </>
  );
}

/** DISPLAY: contrast and density (accent, theme and portrait size stay on APPEARANCE). */
export function DisplayTab({ form, set }: FormProps) {
  const density = useId();
  return (
    <>
      <TabTitle title="DISPLAY" desc="Contrast and spacing of the application." />
      <DescCheck title="High contrast" description="Stronger borders, text and focus rings." checked={form.highContrast} onChange={(v) => set({ highContrast: v })} />
      <FieldLabel htmlFor={density}>DENSITY</FieldLabel>
      <ComboBox id={density} value={form.density} onChange={(e) => set({ density: e.target.value as Density })}
        options={[{ value: "comfortable", label: "Comfortable" }, { value: "compact", label: "Compact" }]} />
    </>
  );
}

/** CONTENT: registered content indexes. The place for Borealis's own folders (GitHub #32 adds the portrait library here). */
export function ContentTab({ manage }: { manage: () => void }) {
  const api = useApi();
  const indexes = useAsync(() => api.listIndexes(), [api]);
  return (
    <>
      <TabTitle title="CONTENT" desc="Content packs and the indexes they are updated from." />
      <FieldLabel>CONTENT INDEXES</FieldLabel>
      <Help>{indexes.data && indexes.data.length === 0 ? "None registered." : "Indexes you added under Additional Content."}</Help>
      {(indexes.data ?? []).map((i) => (
        <div key={i.url} className="aur-set-index"><strong>{i.name}{!i.verified && <UnverifiedBadge />}</strong><Help>{i.url}</Help></div>
      ))}
      <div className="aur-set-buttons"><Button aria-label="Manage…" onClick={manage}>Manage...</Button></div>
    </>
  );
}

/** AI ASSISTANT (builds with the AI CLI integration only): the switch and provider wait for Save; the program path is set at once. */
export function AiTab({ form, set, setup }: FormProps & { setup: () => void }) {
  const api = useApi();
  const { settings: stored, update, replace } = useSettings();
  const [error, setError] = useState<string | null>(null);
  const provider = useId();
  const setAi = (patch: Partial<AppSettings["ai"]>) => set({ ai: { ...form.ai, ...patch } });

  /** The AI program is chosen in the backend's native dialog; the backend validates it and asks for confirmation. */
  const browse = async () => {
    setError(null);
    try {
      const [pick] = await api.pickPath({ purpose: "ai-override" });
      if (pick) replace(await api.setAiOverride({ token: pick.token }));
    } catch (e) { setError(errorMessage(e)); }
  };
  const clear = () => void update({ ai: { ...stored.ai, overridePath: null } }).catch((e: unknown) => setError(errorMessage(e)));

  return (
    <>
      <TabTitle title="AI ASSISTANT" desc="The command line program that answers rules questions and drafts content." />
      <DescCheck title="Enable AI assistant (optional)" description="Uses your own Claude or Codex command-line tool." checked={form.ai.enabled} onChange={(v) => setAi({ enabled: v })} />
      {form.ai.enabled && (
        <>
          <FieldLabel htmlFor={provider}>AI PROVIDER</FieldLabel>
          <ComboBox id={provider} value={form.ai.provider} onChange={(e) => setAi({ provider: e.target.value as AiProviderId })}
            options={[{ value: "claude", label: "Claude CLI" }, { value: "codex", label: "Codex CLI" }, { value: "none", label: "Whichever is installed" }]} />
          <FieldLabel>AI PROGRAM</FieldLabel>
          <Help>{stored.ai.overridePath ?? "Found automatically. Browse to use a different claude or codex program."}</Help>
          <div className="aur-set-buttons">
            <Button aria-label="Browse…" onClick={() => void browse()}>Browse...</Button>
            {stored.ai.overridePath && <Button onClick={clear}>Clear</Button>}
          </div>
          {error && <Alert tone="danger" role="alert">{error}</Alert>}
          <FieldLabel>AI TOOLS</FieldLabel>
          <Help>Check, install or sign in to the Claude and Codex tools.</Help>
          <div className="aur-set-buttons"><Button aria-label="AI setup…" onClick={setup}>AI setup...</Button></div>
        </>
      )}
    </>
  );
}

/** PHONES (while the AI assistant is on): share this computer's AI program with paired phones. */
export function PhonesTab() {
  return (
    <>
      <TabTitle title="PHONES AND TABLETS" desc="Phones and tablets cannot run the AI programs; they connect to a desktop instance." />
      <AiHostPanel />
    </>
  );
}

/** UPDATES (not in store builds): the new-version check; Check now asks at once. */
export function UpdatesTab({ form, set }: FormProps) {
  const api = useApi();
  const [checking, setChecking] = useState(false);
  const { status } = useAppUpdateView();
  const checkNow = async () => { setChecking(true); try { await checkAppUpdateNow(api); } finally { setChecking(false); } };
  return (
    <>
      <TabTitle title="UPDATES" desc="New versions of Borealis." />
      <DescCheck title="Check for new versions of Borealis" description="Only asks GitHub for the latest version number; nothing about you or your characters is sent."
        checked={form.checkAppUpdates} onChange={(v) => set({ checkAppUpdates: v })} />
      <div className="aur-set-buttons">
        <Button disabled={checking} onClick={() => void checkNow()}>Check now</Button>
        {status?.state === "done" && status.newer && <Button onClick={() => void openDownload(api, status.url ?? undefined)}>Download</Button>}
      </div>
      <p className="aur-set-help" role="status">{updateText(status)}</p>
    </>
  );
}

/** The line under "Check now": the last answer in words. */
export function updateText(u: ReturnType<typeof useAppUpdateView>["status"]): string {
  if (!u || u.state === "idle" || u.state === "disabled") return "";
  if (u.state === "failed") return "Could not reach GitHub. Check your connection and try again.";
  return u.newer ? `Borealis ${u.latest} is available (you have ${u.current}).` : `You have the latest version (${u.current}).`;
}
