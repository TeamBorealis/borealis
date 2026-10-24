import { useId, useState } from "react";
import type { SearchSettings, SemanticConsent, SemanticStatus, SemanticTier } from "../../api/contract";
import { Alert, Button, ComboBox, MessageBox } from "../../components";
import { MetroProgressBar } from "../../components/controls/Controls";
import { useApi } from "../../state/ApiContext";
import { megabytes, useSemanticStatus } from "../../state/semantic";
import { useSettings } from "../../state/SettingsContext";
import { setStatus as setStatusBar } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import { DescCheck, FieldLabel, Help, SectionDesc, SectionTitle } from "./rows";

const TIER_LABEL: Record<SemanticTier, string> = {
  t0: "Keyword search only",
  t1: "Tier 1: small model, light indexing",
  t2: "Tier 2: full model",
  t3: "Tier 3: full model, most detail",
};
const TIERS: SemanticTier[] = ["t1", "t2", "t3"];
/** Shown instead of the switch text while no embedding backend is linked (B-46); the backend refuses downloads too. */
export const NO_BACKEND_YET = "Semantic search (search by meaning) arrives in a later update. Search uses keywords until then.";

/** The consent dialog (spec section 8 wording, Aurora message box). Nothing downloads unless the first button is pressed. */
export function SemanticConsentDialog({ consent, onResult }: { consent: SemanticConsent; onResult: (accepted: boolean) => void }) {
  return (
    <MessageBox title={consent.title} message={consent.body} cancelId="decline"
      buttons={[{ id: "accept", label: consent.accept, variant: "primary" }, { id: "decline", label: consent.decline }]}
      onResult={(id) => onResult(id === "accept")} />
  );
}

/** The line under the semantic switch. */
function switchText(st: SemanticStatus): string {
  if (!st.backendAvailable) return NO_BACKEND_YET;
  if (!st.available && !st.modelPresent) return st.unavailableReason ?? "";
  if (st.modelPresent && !st.backendAvailable) return st.download.message ?? "Model downloaded. Semantic search arrives in a later update of Borealis.";
  if (st.active) return "On: search also finds rules, lore and session notes by meaning.";
  if (st.modelPresent) return "The model is on this device. Turn this on to use it.";
  return `Downloads the EmbeddingGemma 2 model (${st.file?.sizeText ?? ""}) once. Everything runs on this device.`;
}

function progressText(st: SemanticStatus): string {
  const d = st.download;
  switch (d.state) {
    case "downloading": return `Downloading the model: ${megabytes(d.done, d.total)}.`;
    case "verifying": return "Checking the downloaded file...";
    case "paused": return d.message ?? `Paused at ${megabytes(d.done, d.total)}.`;
    case "failed": return d.message ?? "The model could not be downloaded.";
    default: return "";
  }
}

/** Borealis Settings > Search: semantic search switch (consent first), download progress, tier override, battery, delete. */
export function SearchTab() {
  const api = useApi();
  const { settings: s, update, replace } = useSettings();
  const { status: st, setStatus, refresh } = useSemanticStatus();
  const [asking, setAsking] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const tierId = useId();
  const search = s.search;
  const setSearch = (patch: Partial<SearchSettings>) => void update({ search: { ...search, ...patch } });

  /** Run an action that returns a status and may change `settings.search.semantic` in the backend. */
  const act = async (call: () => Promise<SemanticStatus>) => {
    setError(null);
    try {
      setStatus(await call());
      replace(await api.getSettings());
    } catch (e) {
      setError(errorMessage(e));
      await refresh();
    }
  };

  if (!st) return <SectionTitle first>SEARCH</SectionTitle>;
  const d = st.download;
  const busy = d.state === "downloading" || d.state === "verifying";
  const resumable = d.state === "paused" || d.state === "failed";

  const toggle = (on: boolean) => {
    if (!st.backendAvailable) return;
    if (!on) {
      if (busy || resumable) void act(() => api.semanticDownloadCancel());
      else setSearch({ semantic: false });
      return;
    }
    if (st.modelPresent) setSearch({ semantic: true });
    else if (st.available) setAsking(true);
  };
  const consented = (accepted: boolean) => {
    setAsking(false);
    if (accepted) void act(() => api.semanticDownloadStart());
  };
  const remove = async () => {
    setDeleting(false);
    await act(() => api.semanticDeleteModel());
    setStatusBar("The semantic search model and its indexes were deleted.");
  };

  const probed = TIERS.indexOf(st.probedTier as (typeof TIERS)[number]);
  const tierOptions = [{ value: "auto", label: `Automatic (${TIER_LABEL[st.probedTier]})` },
    ...TIERS.filter((_, i) => i < probed).map((t) => ({ value: t, label: TIER_LABEL[t] }))];
  const hasFiles = st.modelPresent || (!busy && d.done > 0);

  return (
    <>
      <h3 className="aur-set-title13 first flush">SEARCH</h3>
      <SectionDesc>Keyword search works offline on every device. Semantic search is optional: it downloads a model once and also finds text by meaning.</SectionDesc>
      {!st.file && <Help>{st.unavailableReason}</Help>}
      {st.file && (
        <>
          <DescCheck title="Semantic search (optional)" description={switchText(st)} checked={search.semantic && st.backendAvailable}
            disabled={!st.backendAvailable} onChange={(v) => toggle(v)} />
          {(busy || resumable) && (
            <>
              <FieldLabel>MODEL DOWNLOAD</FieldLabel>
              <MetroProgressBar label="Model download" value={d.done} max={Math.max(d.total, 1)} />
              <p className="aur-set-help" role="status">{progressText(st)}</p>
              <div className="aur-set-buttons">
                {d.state === "downloading" && <Button onClick={() => void act(() => api.semanticDownloadPause())}>Pause</Button>}
                {resumable && st.available && st.backendAvailable && <Button onClick={() => void act(() => api.semanticDownloadStart())}>{d.state === "failed" ? "Try again" : "Resume"}</Button>}
                <Button variant="clear" onClick={() => void act(() => api.semanticDownloadCancel())}>Cancel</Button>
              </div>
              {st.mobile && <Help>The download pauses while Borealis is in the background; press Resume to continue.</Help>}
            </>
          )}
          <FieldLabel htmlFor={tierId}>DEVICE TIER</FieldLabel>
          <ComboBox id={tierId} value={tierOptions.some((o) => o.value === search.tierOverride) ? search.tierOverride : "auto"} onChange={(e) => setSearch({ tierOverride: e.target.value as SearchSettings["tierOverride"] })} options={tierOptions} />
          <Help>Chosen from this device's memory. A lower tier uses a smaller model and fewer threads; it can be lowered, never raised.</Help>
          {st.mobile && (
            <DescCheck spaced title="Index on battery" description="Build the semantic indexes on battery too, while the battery is at least 50%. Otherwise only while charging."
              checked={search.indexOnBattery} onChange={(v) => setSearch({ indexOnBattery: v })} />
          )}
          {hasFiles && (
            <div className="aur-set-left-clear">
              <Button variant="danger" onClick={() => setDeleting(true)}>Delete model and semantic indexes</Button>
            </div>
          )}
        </>
      )}
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
      {asking && st.consent && <SemanticConsentDialog consent={st.consent} onResult={consented} />}
      {deleting && (
        <MessageBox title="Delete model and semantic indexes" cancelId="cancel"
          message="Delete the semantic search model and the semantic indexes from this device? Semantic search turns off; keyword search keeps working. You can download the model again later."
          buttons={[{ id: "delete", label: "Delete", variant: "danger" }, { id: "cancel", label: "Cancel" }]}
          onResult={(id) => { if (id === "delete") void remove(); else setDeleting(false); }} />
      )}
    </>
  );
}
