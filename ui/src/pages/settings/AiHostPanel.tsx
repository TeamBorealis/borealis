import { useCallback, useEffect, useState } from "react";
import type { AiHostStatus } from "../../api/contract";
import { Alert, Button } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useWithCodexConsent } from "../../state/codexConsent";
import { errorMessage } from "../../state/useAsync";
import { QrCode } from "./QrCode";
import { DescCheck, FieldLabel, Help } from "./rows";

const POLL_MS = 15_000;

/** Desktop: share the AI program with paired phones (pairing QR, code and certificate fingerprint). */
export function AiHostPanel() {
  const api = useApi();
  const withConsent = useWithCodexConsent();
  const [host, setHost] = useState<AiHostStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const run = useCallback(async (call: () => Promise<AiHostStatus>) => {
    setError(null);
    try { setHost(await call()); } catch (e) { setError(errorMessage(e)); }
  }, []);

  useEffect(() => { void run(() => api.aiHostStatus()); }, [api, run]);
  // A host stops by itself after 30 minutes without use; look again now and then to notice.
  useEffect(() => {
    if (!host?.enabled) return;
    const t = setInterval(() => void run(() => api.aiHostStatus()), POLL_MS);
    return () => clearInterval(t);
  }, [api, run, host?.enabled]);

  if (!host?.supported) return null;
  const copy = async (text: string) => {
    try { await navigator.clipboard.writeText(text); setCopied(true); } catch { setCopied(false); }
  };
  return (
    <>
      <DescCheck title="Share AI with phones and tablets" description="Paired devices on your network use the AI program on this computer. It stops after 30 minutes without use." checked={host.enabled}
        onChange={(on) => void run(() => (on ? withConsent((allow) => api.aiHostEnable(allow ? { allowUntrustedWithCodex: true } : {})) : api.aiHostDisable()))} />
      {host.enabled && (
        <>
          {host.pairingUri && host.pairingCode ? (
            <div className="aur-set-pair">
              <div>
                <FieldLabel>PAIR A DEVICE</FieldLabel>
                <Help>Scan this code with Borealis on the phone, or type the digits: <strong>{host.pairingCode}</strong>. Valid for about {Math.max(1, Math.round((host.pairingExpiresInSecs ?? 0) / 60))} minutes.</Help>
                {host.fingerprint && <Help>Check the phone shows this fingerprint: <code>{host.fingerprint}</code></Help>}
                <Button small onClick={() => void copy(host.pairingUri ?? "")}>{copied ? "Copied" : "Copy pairing text"}</Button>
              </div>
              <QrCode value={host.pairingUri} label="Pairing QR code" />
            </div>
          ) : (
            <>
              <FieldLabel>PAIR A DEVICE</FieldLabel>
              {host.fingerprint && <Help>Certificate fingerprint {host.fingerprint}</Help>}
              <Button onClick={() => void run(() => api.aiHostBeginPairing())}>New pairing code</Button>
            </>
          )}
          {host.devices.map((d) => (
            <div key={d.id} className="aur-set-device">
              <span><strong>{d.name}</strong><Help>Paired device</Help></span>
              <Button onClick={() => void run(() => api.aiHostRevoke({ deviceId: d.id }))}>Revoke</Button>
            </div>
          ))}
        </>
      )}
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
    </>
  );
}
