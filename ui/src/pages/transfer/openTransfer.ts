/** Which way the data goes: the button the user pressed decides (spec 4). */
export type TransferRole = "send" | "receive";

export interface TransferRequest {
  role: TransferRole;
  /** Send: characters ticked when the dialog opens. */
  characterIds?: string[];
}

const OPEN_EVENT = "borealis:transfer";

/** File > Send to device... (desktop) / More > Send to device (phones). */
export function openSendToDevice(characterIds: string[] = []): void {
  window.dispatchEvent(new CustomEvent<TransferRequest>(OPEN_EVENT, { detail: { role: "send", characterIds } }));
}

/** File > Receive from device... (desktop) / More > Receive from device (phones). */
export function openReceiveFromDevice(): void {
  window.dispatchEvent(new CustomEvent<TransferRequest>(OPEN_EVENT, { detail: { role: "receive" } }));
}

/** Subscribe to open requests (the shell's TransferHost). */
export function onTransferRequest(handler: (r: TransferRequest) => void): () => void {
  const on = (e: Event) => handler((e as CustomEvent<TransferRequest>).detail);
  window.addEventListener(OPEN_EVENT, on);
  return () => window.removeEventListener(OPEN_EVENT, on);
}
