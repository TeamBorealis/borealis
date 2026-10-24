/** Which way the data goes: the button the user pressed decides (spec 4). */
export type TransferRole = "send" | "receive";

/** The transfer dialog also exports to a file: "export" writes a bundle (desktop) or shares it as a file (phones). */
export type TransferMode = TransferRole | "export";

export interface TransferRequest {
  role: TransferMode;
  /** Send / export: characters ticked when the dialog opens. */
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

/** File > Export to bundle... (desktop) / More > Share as a file (phones): pick, then write or share a `.borealis` file. */
export function openExportBundle(characterIds: string[] = []): void {
  window.dispatchEvent(new CustomEvent<TransferRequest>(OPEN_EVENT, { detail: { role: "export", characterIds } }));
}

/** Subscribe to open requests (the shell's TransferHost). */
export function onTransferRequest(handler: (r: TransferRequest) => void): () => void {
  const on = (e: Event) => handler((e as CustomEvent<TransferRequest>).detail);
  window.addEventListener(OPEN_EVENT, on);
  return () => window.removeEventListener(OPEN_EVENT, on);
}
