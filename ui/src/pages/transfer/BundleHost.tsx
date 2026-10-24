import { useCallback, useEffect, useRef, useState } from "react";
import type { BundlePreview } from "../../api/contract";
import { Alert, Button, Dialog, MessageBox } from "../../components";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import { bundleOpenedLine, formatBytes } from "./messages";
import { announceBundleApplied, onBundleRequest, type BundleRequest } from "./openBundle";
import { PreviewList } from "./TransferViews";
import "../settings/settings.css";
import "../ai/panes.css";
import "./transfer.css";

type Shown =
  | { kind: "preview"; bundle: BundlePreview }
  | { kind: "refused"; name: string; message: string };

const itemsText = (n: number): string => (n === 1 ? "1 item" : `${n} items`);

/**
 * Opening a `.borealis` file (File > Open bundle..., a drop, the OS "open with"): the same preview a device transfer
 * shows (new / replaces your older copy / yours is newer: kept), then Accept applies it through the validated path.
 * Files that arrive together are shown one after the other. Nothing is written until Accept.
 */
export function BundleHost() {
  const api = useApi();
  const [queue, setQueue] = useState<BundleRequest[]>([]);
  const [shown, setShown] = useState<Shown | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const loading = useRef(false);

  useEffect(() => onBundleRequest((r) => setQueue((q) => [...q, r])), []);

  // Open the next queued file once the previous dialog closed.
  useEffect(() => {
    const next = queue[0];
    if (!next || shown || loading.current) return;
    loading.current = true;
    setQueue((q) => q.slice(1));
    api.bundleOpen({ token: next.token })
      .then((bundle) => setShown({ kind: "preview", bundle }))
      .catch((e) => setShown({ kind: "refused", name: next.name, message: errorMessage(e) }))
      .finally(() => { loading.current = false; });
  }, [api, queue, shown]);

  const close = useCallback((discard: boolean) => {
    if (discard && shown?.kind === "preview") void api.bundleDiscard({ bundleId: shown.bundle.bundleId }).catch(() => undefined);
    setError(null);
    setShown(null);
  }, [api, shown]);

  const accept = async () => {
    if (shown?.kind !== "preview") return;
    setBusy(true);
    setError(null);
    try {
      const summary = await api.bundleAccept({ bundleId: shown.bundle.bundleId });
      setStatus(bundleOpenedLine(shown.bundle.fileName, summary));
      announceBundleApplied();
      setShown(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (!shown) return null;
  if (shown.kind === "refused") {
    return (
      <MessageBox title="Open bundle" cancelId="ok" buttons={[{ id: "ok", label: "OK", variant: "highlight" }]}
        message={<><strong>{shown.name}</strong><br />{shown.message}</>}
        onResult={() => close(false)} />
    );
  }
  const { bundle } = shown;
  const count = bundle.preview.items.length;
  const cancel = () => { setStatus("Open bundle cancelled. Nothing was changed."); close(true); };
  return (
    <Dialog title="Open bundle" onClose={cancel} className="xfer-dlg"
      footer={[
        <Button key="accept" variant="highlight" disabled={busy} onClick={() => void accept()} data-autofocus>Accept</Button>,
        <Button key="cancel" disabled={busy} onClick={cancel}>Cancel</Button>,
      ]}>
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
      <p className="xfer-instr">
        <strong>{bundle.fileName}</strong> from {bundle.sourceDevice} holds {itemsText(count)} ({formatBytes(bundle.preview.totalBytes)}):
      </p>
      <PreviewList preview={bundle.preview} />
    </Dialog>
  );
}
