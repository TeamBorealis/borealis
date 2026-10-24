import { useState } from "react";
import type { LinkDevice } from "../../api/contract";
import { Alert, Button, MessageBox } from "../../components";
import { useApi } from "../../state/ApiContext";
import { usePlatform } from "../../state/platform";
import { setStatus } from "../../state/status";
import { errorMessage, useAsync } from "../../state/useAsync";
import { formatDay, PLATFORM_LABEL } from "../transfer/messages";
import { Help, SectionDesc } from "./rows";

/** Borealis Settings > Devices (C1): paired devices with last sync and expiry; Remove revokes one. */
export function DevicesTab() {
  const api = useApi();
  const { mobile } = usePlatform();
  const devices = useAsync(() => api.linkDevices(), [api]);
  const [removing, setRemoving] = useState<LinkDevice | null>(null);
  const [error, setError] = useState<string | null>(null);
  const where = mobile ? "More > Send to device or Receive from device" : "File > Send to device... or Receive from device...";

  const remove = async (d: LinkDevice) => {
    setError(null);
    try {
      devices.setData(await api.linkForgetDevice({ deviceId: d.id }));
      setStatus(`Removed '${d.name}' from your devices.`);
    } catch (e) { setError(errorMessage(e)); }
  };

  const list = devices.data ?? [];
  return (
    <>
      <h3 className="aur-set-title13 first flush">DEVICES</h3>
      <SectionDesc>Devices this one sent characters to or received them from, on the same Wi-Fi. A device is forgotten 180 days after its last transfer.</SectionDesc>
      {devices.error && <Alert tone="danger" role="alert">{devices.error}</Alert>}
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
      {devices.data && list.length === 0 && <Help>No devices yet. Use {where} to connect one with a QR code or a typed code.</Help>}
      {list.map((d) => (
        <div key={d.id} className="aur-set-device" aria-label={d.name} role="group">
          <span>
            <strong>{d.name}</strong>
            <Help>{PLATFORM_LABEL[d.platform]} · Last synced {formatDay(d.lastSyncedAt)} · Expires {formatDay(d.expiresAt)}</Help>
          </span>
          <Button onClick={() => setRemoving(d)}>Remove</Button>
        </div>
      ))}
      {list.length > 0 && <Help>Remove a lost or old device: it is refused from then on, until you connect it again with a new code shown here.</Help>}
      {removing && (
        <MessageBox title="Remove device" cancelId="cancel"
          message={`Remove '${removing.name}'? It can no longer connect to this device until you connect it again with a new code.`}
          buttons={[{ id: "remove", label: "Remove", variant: "danger" }, { id: "cancel", label: "Cancel" }]}
          onResult={(id) => { const d = removing; setRemoving(null); if (id === "remove") void remove(d); }} />
      )}
    </>
  );
}
