import { useEffect, useState } from "react";
import { onTransferRequest, type TransferRequest } from "./openTransfer";
import { TransferDialog } from "./TransferDialog";

/** Shell mount point for the Send / Receive dialogs (opened from the File menu, More, or a character tile). */
export function TransferHost() {
  const [open, setOpen] = useState<(TransferRequest & { n: number }) | null>(null);
  useEffect(() => onTransferRequest((r) => setOpen((cur) => cur ?? { ...r, n: Date.now() })), []);
  if (!open) return null;
  return <TransferDialog key={open.n} role={open.role} initialIds={open.characterIds ?? []} onClose={() => setOpen(null)} />;
}
