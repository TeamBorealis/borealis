import { type ReactNode, createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import { useApi } from "../state/ApiContext";
import { useSettings } from "../state/SettingsContext";
import { Button } from "./Button";
import { Check } from "./Fields";
import { Modal } from "./Modal";

export const IMPORT_NOTICE_TITLE = "Import only content you own";
export const IMPORT_NOTICE_TEXT =
  "Only import content that you own or have a licence to use, such as books you have purchased, your own homebrew, or openly licensed material like the SRD. Imported content stays on this device and is for your personal use. Borealis does not provide content, does not condone importing material in breach of copyright or licence terms, and is not responsible for what you choose to import. By continuing you confirm that you have the right to use this content.";

interface Api {
  /** Resolves true when the user may proceed (notice switched off, or "I understand, continue"); false on Cancel. Call before any picker or read. */
  ensure: () => Promise<boolean>;
  /** Show the notice for reading only (no checkbox, no side effects). */
  read: () => void;
}
const Ctx = createContext<Api>({ ensure: () => Promise.resolve(true), read: () => undefined });

type Mode = "gate" | "read";

export function ImportNoticeProvider({ children }: { children: ReactNode }) {
  const api = useApi();
  const { settings, loaded, update } = useSettings();
  const [mode, setMode] = useState<Mode | null>(null);
  const [skip, setSkip] = useState(false);
  const resolver = useRef<((ok: boolean) => void) | null>(null);
  const shouldShow = settings.showImportNotice;

  const ensure = useCallback(async () => {
    // A file opened at launch can arrive before the stored preference has loaded: ask the backend rather than assume the default.
    const show = loaded ? shouldShow : await api.getSettings().then((s) => s.showImportNotice, () => true);
    if (!show) return true;
    return new Promise<boolean>((resolve) => { resolver.current = resolve; setSkip(false); setMode("gate"); });
  }, [api, loaded, shouldShow]);
  const read = useCallback(() => setMode("read"), []);
  const finish = (ok: boolean) => {
    const wasGate = mode === "gate";
    setMode(null);
    if (!wasGate) return;
    if (ok && skip) void update({ showImportNotice: false }).catch(() => undefined);
    resolver.current?.(ok);
    resolver.current = null;
  };
  const value = useMemo(() => ({ ensure, read }), [ensure, read]);
  return (
    <Ctx.Provider value={value}>
      {children}
      {mode && (
        <Modal title={IMPORT_NOTICE_TITLE} onClose={() => finish(false)}
          footer={mode === "gate"
            ? <><Button onClick={() => finish(false)}>Cancel</Button><Button variant="primary" data-autofocus onClick={() => finish(true)}>I understand, continue</Button></>
            : <Button variant="primary" data-autofocus onClick={() => finish(false)}>Close</Button>}>
          <div className="stack">
            <p>{IMPORT_NOTICE_TEXT}</p>
            {mode === "gate" && <Check label="Don't show this again" checked={skip} onChange={(e) => setSkip(e.target.checked)} />}
          </div>
        </Modal>
      )}
    </Ctx.Provider>
  );
}

export const useImportNotice = (): Api => useContext(Ctx);
