import { type ReactNode, useCallback, useState } from "react";
import { MessageBox, type MessageBoxButton } from "../../components";

interface Ask { title: string; message: ReactNode; buttons: MessageBoxButton[]; cancelId: string }
interface Pending extends Ask { resolve: (id: string) => void }

/** Aurora's MessageBox as a promise (13.12): `ask` resolves with the button id; Escape resolves with `cancelId`. */
export function useMessageBox(): { ask: (a: Ask) => Promise<string>; box: ReactNode } {
  const [pending, setPending] = useState<Pending | null>(null);
  const ask = useCallback((a: Ask) => new Promise<string>((resolve) => setPending({ ...a, resolve })), []);
  const box = pending && (
    <MessageBox title={pending.title} message={pending.message} buttons={pending.buttons} cancelId={pending.cancelId}
      onResult={(id) => { pending.resolve(id); setPending(null); }} />
  );
  return { ask, box };
}
