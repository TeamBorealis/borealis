import { type ReactElement, useEffect, useState } from "react";
import { useNavigate } from "react-router";
import type { ContentPack } from "../../api/contract";
import { Alert, Button, ComboBox, Dialog } from "../../components";
import { CheckBox } from "../../components/controls";
import { useApi } from "../../state/ApiContext";
import { usePlatform } from "../../state/platform";
import { errorMessage, useAsync } from "../../state/useAsync";
import { NewPackFlyout } from "./NewPackFlyout";
import { lastPackId, rememberPack } from "./packs";

/** The element to copy: its id, name and whether it is a class (a class offers "Copy its features and subclasses too"). */
export interface CopyTarget {
  id: string;
  name: string;
  isClass: boolean;
}

/** HC20: a variant (a copy with its own id, spec 11) or an override (the original changed in place, spec 22). */
export type CopyMode = "variant" | "override";

const MODES: { id: CopyMode; label: string; help: string }[] = [
  { id: "variant", label: "Make a variant (copy)", help: "A new element of your own. The original stays as it is." },
  { id: "override", label: "Override the original", help: "Change it in place. Characters that use it get the change." },
];

/**
 * Copy to homebrew (spec 2, 11), from the Compendium and the builder's description pane: choose how (a variant, or since
 * HC20 an override of the original, spec 22) and the homebrew pack (or make one when there is none), then open it in the
 * creator. A copy keeps the element's rules, tags, description and edition, with a new id; a class copies its features and
 * subclasses too unless unticked. An override keeps the original's id and replaces it everywhere once saved.
 */
export function CopyToHomebrew({ target, onClose, onDone }: { target: CopyTarget; onClose: () => void; onDone?: () => void }) {
  const api = useApi();
  const nav = useNavigate();
  const packs = useAsync(() => api.listPacks(), [api]);
  const own = (packs.data ?? []).filter((p) => p.kind === "homebrew");
  const [packId, setPackId] = useState<string>("");
  const [deep, setDeep] = useState(true);
  const [mode, setMode] = useState<CopyMode>("variant");
  const [making, setMaking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (packId || own.length === 0) return;
    const last = lastPackId();
    setPackId(own.some((p) => p.id === last) ? last! : own[0]!.id);
  }, [own, packId]);

  const copy = async (into: string) => {
    setBusy(true);
    try {
      const view = mode === "override"
        ? await api.homebrewNewOverride({ packId: into, elementId: target.id })
        : await api.homebrewNewDraft({ packId: into, kind: "other", copyFrom: target.id, deepCopy: target.isClass ? deep : true });
      rememberPack(into);
      onDone?.();
      onClose();
      nav(`/homebrew/${encodeURIComponent(into)}?draft=${encodeURIComponent(view.draftId)}`);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const created = (pack: ContentPack) => { setMaking(false); void copy(pack.id); };

  if (making) return <NewPackFlyout onCreated={created} onClose={() => setMaking(false)} />;
  const none = packs.data !== undefined && own.length === 0;
  const footer = none
    ? <><Button onClick={onClose}>Cancel</Button><Button variant="primary" onClick={() => setMaking(true)}>New pack</Button></>
    : <><Button onClick={onClose}>Cancel</Button><Button variant="primary" data-testid="copy-to-homebrew" disabled={busy || !packId} onClick={() => void copy(packId)}>{mode === "override" ? "Override" : "Copy"}</Button></>;
  return (
    <Dialog title="Copy to homebrew" onClose={onClose} footer={footer} className="hb-copy-dialog">
      {packs.error && <Alert tone="danger">{packs.error}</Alert>}
      {none ? (
        <p>You have no homebrew pack yet. Make one to copy or change “{target.name}” in.</p>
      ) : (
        <>
          <p>Change “{target.name}” in a homebrew pack.</p>
          <fieldset className="hb-copy-mode">
            <legend className="v4-fl">How</legend>
            {MODES.map((m) => (
              <label key={m.id} className="radio-row hb-copy-mode-row">
                <input type="radio" name="hb-copy-mode" value={m.id} checked={mode === m.id} onChange={() => setMode(m.id)} />
                <span><span className="hb-copy-mode-label">{m.label}</span><span className="hb-field-help">{m.help}</span></span>
              </label>
            ))}
          </fieldset>
          <label className="v4-fl" htmlFor="hb-copy-pack">Pack</label>
          <ComboBox id="hb-copy-pack" value={packId} onChange={setPackId} options={own.map((p) => ({ value: p.id, label: p.name }))} />
          {target.isClass && mode === "variant" && (
            <label className="hb-inline-check"><CheckBox checked={deep} onChange={(e) => setDeep(e.target.checked)} />Copy its features and subclasses too</label>
          )}
        </>
      )}
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
    </Dialog>
  );
}

/** "Copy to homebrew" as a pane command: shown only while the creator is on (HC3). */
export function useCopyToHomebrew(onDone?: () => void): { enabled: boolean; open: (t: CopyTarget) => void; dialog: ReactElement | null } {
  const { creatorEnabled } = usePlatform();
  const [target, setTarget] = useState<CopyTarget | null>(null);
  return {
    enabled: creatorEnabled,
    open: setTarget,
    dialog: target ? <CopyToHomebrew target={target} onClose={() => setTarget(null)} onDone={onDone} /> : null,
  };
}
