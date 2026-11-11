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

/**
 * Copy to homebrew (spec 2, 11), from the Compendium and the builder's description pane: choose the homebrew pack (or make
 * one when there is none), copy, and open the copy in the creator with its name selected for renaming. The copy keeps the
 * element's rules, tags, description and edition; a class copies its features and subclasses too unless unticked.
 */
export function CopyToHomebrew({ target, onClose, onDone }: { target: CopyTarget; onClose: () => void; onDone?: () => void }) {
  const api = useApi();
  const nav = useNavigate();
  const packs = useAsync(() => api.listPacks(), [api]);
  const own = (packs.data ?? []).filter((p) => p.kind === "homebrew");
  const [packId, setPackId] = useState<string>("");
  const [deep, setDeep] = useState(true);
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
      const view = await api.homebrewNewDraft({ packId: into, kind: "other", copyFrom: target.id, deepCopy: target.isClass ? deep : true });
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
    : <><Button onClick={onClose}>Cancel</Button><Button variant="primary" data-testid="copy-to-homebrew" disabled={busy || !packId} onClick={() => void copy(packId)}>Copy</Button></>;
  return (
    <Dialog title="Copy to homebrew" onClose={onClose} footer={footer} className="hb-copy-dialog">
      {packs.error && <Alert tone="danger">{packs.error}</Alert>}
      {none ? (
        <p>You have no homebrew pack yet. Make one to copy “{target.name}” into.</p>
      ) : (
        <>
          <p>Copy “{target.name}” into a homebrew pack to change it. The original stays as it is.</p>
          <label className="v4-fl" htmlFor="hb-copy-pack">Pack</label>
          <ComboBox id="hb-copy-pack" value={packId} onChange={setPackId} options={own.map((p) => ({ value: p.id, label: p.name }))} />
          {target.isClass && (
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
