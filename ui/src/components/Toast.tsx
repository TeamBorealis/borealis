import { type ReactNode, createContext, useCallback, useContext, useMemo, useState } from "react";
import { isPhone } from "../lib/phone";

type Tone = "info" | "error" | "warning";
interface ToastItem { id: number; text: string; tone: Tone }
interface ToastApi { push: (text: string, tone?: Tone) => void }

const Ctx = createContext<ToastApi>({ push: () => undefined });
let seq = 0;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const dismiss = useCallback((id: number) => setItems((cur) => cur.filter((t) => t.id !== id)), []);
  const push = useCallback((text: string, tone: Tone = "info") => {
    const id = ++seq;
    setItems((cur) => [...cur.slice(-3), { id, text, tone }]);
    // Phones show plain toasts for 4 s (B-48 spec sec. 3.4); errors stay 8 s so they can be read.
    setTimeout(() => dismiss(id), tone === "error" ? 8000 : isPhone() ? 4000 : 4500);
  }, [dismiss]);
  const api = useMemo(() => ({ push }), [push]);
  return (
    <Ctx.Provider value={api}>
      {children}
      <div className="toasts" role="status" aria-live="polite" aria-atomic="false">
        {items.map((t) => (
          <div key={t.id} className={`toast ${t.tone === "info" ? "" : t.tone}`} role={t.tone === "error" ? "alert" : undefined}>
            <span style={{ flex: 1 }}>{t.text}</span>
            <button type="button" aria-label="Dismiss" onClick={() => dismiss(t.id)}>x</button>
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}

export const useToast = (): ToastApi => useContext(Ctx);
