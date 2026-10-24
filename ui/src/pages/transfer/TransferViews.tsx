import { useId } from "react";
import type { CharacterSummary, LinkPackChoice, LinkPreview, LinkPreviewItem, LinkStatus } from "../../api/contract";
import { Alert, CheckBox, ComboBox, Icon, MetroProgressBar, TextBox, triState } from "../../components";
import { QrCode } from "../settings/QrCode";
import { FieldLabel, Help } from "../settings/rows";
import { ACTION_LABEL, failureText, formatBytes, formatCountdown } from "./messages";
import type { TransferRole } from "./openTransfer";

const PACK_OPTIONS: { value: LinkPackChoice; label: string }[] = [
  { value: "required", label: "Packs these characters use (recommended)" },
  { value: "all", label: "All my content packs" },
  { value: "none", label: "No content packs" },
];

/** Send, step 1: tick characters, choose which packs travel. */
export function PickView({ characters, chosen, onChosen, packs, onPacks, unsavedName, intro }: {
  characters: CharacterSummary[] | null; chosen: ReadonlySet<string>; onChosen: (next: Set<string>) => void;
  packs: LinkPackChoice; onPacks: (p: LinkPackChoice) => void; unsavedName: string | null;
  /** The sentence above the list (Send explains the other device's preview; export explains the file). */
  intro?: string;
}) {
  const packsId = useId();
  const list = characters ?? [];
  const all = triState(list.filter((c) => chosen.has(c.id)).length, list.length);
  const toggle = (id: string, on: boolean) => {
    const next = new Set(chosen);
    if (on) next.add(id); else next.delete(id);
    onChosen(next);
  };
  return (
    <>
      <p className="xfer-instr">{intro ?? "Choose what to send. The other device lists what will change before anything is written."}</p>
      <FieldLabel>CHARACTERS</FieldLabel>
      {characters === null ? <Help>Loading your characters...</Help> : list.length === 0 ? <Help>There are no saved characters yet.</Help> : (
        <div className="xfer-pick" role="group" aria-label="Characters to send">
          <label className="xfer-pick-row all">
            <CheckBox checked={all.checked} mixed={all.mixed} onChange={(e) => onChosen(e.target.checked ? new Set(list.map((c) => c.id)) : new Set())} aria-label="All characters" />
            <span className="xfer-pick-name">All characters</span>
          </label>
          {list.map((c) => (
            <label key={c.id} className="xfer-pick-row">
              <CheckBox checked={chosen.has(c.id)} onChange={(e) => toggle(c.id, e.target.checked)} aria-label={c.name} />
              <span className="xfer-pick-name">{c.name}</span>
              <span className="xfer-pick-build">{c.build}</span>
            </label>
          ))}
        </div>
      )}
      {unsavedName && chosen.size > 0 && <Help>'{unsavedName}' has unsaved changes: save first to send them.</Help>}
      <FieldLabel htmlFor={packsId}>CONTENT PACKS</FieldLabel>
      <ComboBox id={packsId} value={packs} onChange={(e) => onPacks(e.target.value as LinkPackChoice)} options={PACK_OPTIONS} />
      <Help>Homebrew and imported packs travel only between your own devices.</Help>
    </>
  );
}

/** Phones: how to connect (scan is the primary path, spec 4). */
export function MethodView({ role, onScan, onShow, onType }: { role: TransferRole; onScan: () => void; onShow: () => void; onType: () => void }) {
  const other = role === "receive" ? "Send to device" : "Receive from device";
  return (
    <>
      <p className="xfer-instr">On the other device, choose {other}. Then connect the two:</p>
      <ul className="v5-list xfer-methods" aria-label="How to connect">
        <li><button type="button" className="v5-row" onClick={onScan}><Icon name="qr-scan" /><span className="grow">Scan the code on the other device</span><Icon name="chevron" /></button></li>
        <li><button type="button" className="v5-row" onClick={onShow}><Icon name="qr" /><span className="grow">Show a code on this device</span><Icon name="chevron" /></button></li>
        <li><button type="button" className="v5-row" onClick={onType}><Icon name="keyboard" /><span className="grow">Type the code</span><Icon name="chevron" /></button></li>
      </ul>
    </>
  );
}

function showInstruction(role: TransferRole, mobile: boolean): string {
  if (role === "receive") {
    return mobile
      ? "On the other device, choose Send to device, pick what to send, then scan this code (or type the code and address)."
      : "On your phone, open Borealis, choose More > Send to device, pick what to send and scan this code. On another computer, choose File > Send to device... and type the code.";
  }
  return mobile
    ? "On the other device, choose Receive from device, then scan this code (or type the code and address)."
    : "On your phone, open Borealis, choose More > Receive from device and scan this code. On another computer, choose File > Receive from device..., Enter a code, and type this code.";
}

/** The listening side: QR on the left, the code, address and fingerprint below it. */
export function ShowView({ status, remaining, role, mobile }: { status: LinkStatus; remaining: number | null; role: TransferRole; mobile: boolean }) {
  return (
    <div className="xfer-show">
      <div className="xfer-qr-col">
        {status.qrPayload && <QrCode value={status.qrPayload} label="Connection QR code" size={168} />}
        <FieldLabel>CODE</FieldLabel>
        <div className="xfer-code" aria-label="Code">{status.code}</div>
        {status.address && <><FieldLabel>ADDRESS</FieldLabel><div className="xfer-mono">{status.address}</div></>}
        {status.fingerprint && <><FieldLabel>FINGERPRINT</FieldLabel><div className="xfer-mono" title="This device's certificate">{status.fingerprint}</div></>}
      </div>
      <div className="xfer-side">
        <p className="xfer-instr">{showInstruction(role, mobile)}</p>
        <p className="xfer-wait" role="status">
          Waiting for the other device...{remaining !== null && <> The code works once and expires in <strong>{formatCountdown(remaining)}</strong>.</>}
        </p>
        <MetroProgressBar label="Waiting for the other device" />
        <Help>Both devices must be on the same Wi-Fi. Borealis listens only while this window is open.</Help>
      </div>
    </div>
  );
}

/** Typed code (no camera, PC to PC) plus the address the other device shows. */
export function TypeView({ code, onCode, host, onHost, needsHost, notice, error, onSubmit }: {
  code: string; onCode: (v: string) => void; host: string; onHost: (v: string) => void; needsHost: boolean;
  notice: string | null; error: string | null; onSubmit: () => void;
}) {
  const codeId = useId();
  const hostId = useId();
  return (
    <form className="xfer-type" onSubmit={(e) => { e.preventDefault(); onSubmit(); }}>
      {notice && <Alert tone="info">{notice}</Alert>}
      <p className="xfer-instr">Type the code shown under the QR code on the other device.</p>
      <FieldLabel htmlFor={codeId}>CODE</FieldLabel>
      <TextBox id={codeId} value={code} onChange={(e) => onCode(e.target.value)} placeholder="42-ember-lantern" autoComplete="off" autoCapitalize="none" spellCheck={false} data-autofocus />
      <FieldLabel htmlFor={hostId}>{needsHost ? "ADDRESS" : "ADDRESS (OPTIONAL)"}</FieldLabel>
      <TextBox id={hostId} value={host} onChange={(e) => onHost(e.target.value)} placeholder="192.168.1.20:41234" autoComplete="off" inputMode="url" spellCheck={false} />
      <Help>{needsHost ? "Also shown under the QR code. Phones need it to find the other device." : "Only needed when the other device is not found on this network by its code."}</Help>
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
      <button type="submit" hidden aria-hidden tabIndex={-1} />
    </form>
  );
}

function PreviewRow({ item }: { item: LinkPreviewItem }) {
  return (
    <li className={`xfer-prev-row ${item.action}`}>
      <Icon name={item.kind === "character" ? "users" : "box"} />
      <span className="xfer-prev-name">{item.label}{item.missingPacks.length > 0 && <span className="xfer-prev-miss">Needs content not on this device: {item.missingPacks.join(", ")}</span>}</span>
      <span className="xfer-prev-size">{formatBytes(item.size)}</span>
      <span className="xfer-prev-act">{ACTION_LABEL[item.action]}</span>
    </li>
  );
}

const peerName = (s: LinkStatus) => s.peerName ?? "The other device";

/** What Accept would do, one row per item: shared by the device-transfer preview and the open-bundle preview. */
export function PreviewList({ preview }: { preview: LinkPreview }) {
  return (
    <>
      <ul className="xfer-prev" aria-label="What Accept changes">{preview.items.map((i) => <PreviewRow key={`${i.kind}:${i.key}`} item={i} />)}</ul>
      <Help>The most recently edited copy wins. A replaced copy stays in the sync trash for 30 days.</Help>
    </>
  );
}

/** Connected: connecting, waiting for the other side, the receiver's preview, progress, or what failed. */
export function SessionView({ status }: { status: LinkStatus }) {
  const peer = peerName(status);
  switch (status.phase) {
    case "connecting":
      return <><p className="xfer-wait" role="status">Connecting to {status.peerName ?? "the other device"}...</p><MetroProgressBar label="Connecting" /></>;
    case "waiting_for_peer":
      return (
        <>
          <p className="xfer-wait" role="status">Waiting for {peer} to accept...</p>
          <MetroProgressBar label="Waiting for the other device" />
          <Help>They see what will change before anything is written.</Help>
        </>
      );
    case "preview": {
      const items = status.preview?.items ?? [];
      return (
        <>
          <p className="xfer-instr"><strong>{peer}</strong> wants to send {items.length === 1 ? "1 item" : `${items.length} items`} ({formatBytes(status.preview?.totalBytes ?? 0)}):</p>
          <PreviewList preview={{ items, totalBytes: status.preview?.totalBytes ?? 0 }} />
        </>
      );
    }
    case "transferring":
    case "applying": {
      const done = status.bytesDone ?? 0;
      const total = status.bytesTotal ?? 0;
      const verb = status.phase === "applying" ? "Saving" : status.role === "receive" ? `Receiving from ${status.peerName ?? "the other device"}` : `Sending to ${status.peerName ?? "the other device"}`;
      return (
        <>
          <p className="xfer-wait" role="status">{verb}...{total > 0 && ` ${formatBytes(done)} of ${formatBytes(total)}`}</p>
          <MetroProgressBar label="Transfer progress" value={total > 0 ? done : undefined} max={total || 1} />
        </>
      );
    }
    case "failed":
      return <Alert tone="danger" role="alert">Transfer failed: {failureText(status)}</Alert>;
    case "expired":
      return <p className="xfer-wait" role="status">This code expired before another device connected. Show a new code to try again.</p>;
    default:
      return null;
  }
}
