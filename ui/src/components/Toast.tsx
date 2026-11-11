import { type ReactNode, createContext, useCallback, useContext, useMemo, useState } from "react";
import { isPhone } from "../lib/phone";

type Tone = "info" | "error" | "warning";
/** One button on a toast ("Deleted Fireball Plus. Undo", homebrew creator spec 10.2): it runs once and closes the toast. */
export interface ToastAction { label: string; run: () => void }
interface ToastItem { id: number; text: string; tone: Tone; action?: ToastAction }
interface ToastApi { push: (text: string, tone?: Tone, action?: ToastAction) => void }

/** How long a toast with an action stays (spec 10.2: the Undo toast lasts 8 s). */
const ACTION_MS = 8000;

const Ctx = createContext<ToastApi>({ push: () => undefined });
let seq = 0;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const dismiss = useCallback((id: number) => setItems((cur) => cur.filter((t) => t.id !== id)), []);
  const push = useCallback((text: string, tone: Tone = "info", action?: ToastAction) => {
    const id = ++seq;
    setItems((cur) => [...cur.slice(-3), { id, text, tone, ...(action ? { action } : {}) }]);
    // Phones show plain toasts for 4 s (B-48 spec sec. 3.4); errors and toasts with a button stay 8 s.
    setTimeout(() => dismiss(id), tone === "error" || action ? ACTION_MS : isPhone() ? 4000 : 4500);
  }, [dismiss]);
  const api = useMemo(() => ({ push }), [push]);
  return (
    <Ctx.Provider value={api}>
      {children}
      <div className="toasts" role="status" aria-live="polite" aria-atomic="false">
        {items.map((t) => (
          <div key={t.id} className={`toast ${t.tone === "info" ? "" : t.tone}`} role={t.tone === "error" ? "alert" : undefined}>
            <span style={{ flex: 1 }}>{t.text}</span>
            {t.action && <button type="button" className="toast-action" onClick={() => { dismiss(t.id); t.action!.run(); }}>{t.action.label}</button>}
            <button type="button" aria-label="Dismiss" onClick={() => dismiss(t.id)}>x</button>
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}

export const useToast = (): ToastApi => useContext(Ctx);
