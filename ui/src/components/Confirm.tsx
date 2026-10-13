import { type ReactNode, createContext, useCallback, useContext, useMemo, useState } from "react";
import { Button } from "./Button";
import { Modal } from "./Modal";

export interface ConfirmOptions { title: string; body: ReactNode; confirmLabel?: string; danger?: boolean; cancelLabel?: string; extra?: { label: string; value: string } }
type Result = "confirm" | "cancel" | string;
interface Pending extends ConfirmOptions { resolve: (r: Result) => void }

const Ctx = createContext<(o: ConfirmOptions) => Promise<Result>>(() => Promise.resolve("cancel"));

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<Pending | null>(null);
  const ask = useCallback((o: ConfirmOptions) => new Promise<Result>((resolve) => setPending({ ...o, resolve })), []);
  const done = (r: Result) => { pending?.resolve(r); setPending(null); };
  const value = useMemo(() => ask, [ask]);
  return (
    <Ctx.Provider value={value}>
      {children}
      {pending && (
        <Modal title={pending.title} onClose={() => done("cancel")}
          footer={<>
            <Button onClick={() => done("cancel")}>{pending.cancelLabel ?? "Cancel"}</Button>
            {pending.extra && <Button onClick={() => done(pending.extra!.value)}>{pending.extra.label}</Button>}
            <Button variant={pending.danger ? "danger" : "primary"} data-autofocus onClick={() => done("confirm")}>{pending.confirmLabel ?? "Confirm"}</Button>
          </>}>
          {pending.body}
        </Modal>
      )}
    </Ctx.Provider>
  );
}

/** Returns a promise-based confirm dialog. Resolves "confirm", "cancel" or the `extra` value. */
export const useConfirm = (): ((o: ConfirmOptions) => Promise<Result>) => useContext(Ctx);
