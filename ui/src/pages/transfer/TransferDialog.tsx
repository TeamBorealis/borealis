import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import type { LinkPackChoice, LinkSelection } from "../../api/contract";
import { Alert, Button, Dialog, MessageBox } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { usePlatform } from "../../state/platform";
import { setStatus } from "../../state/status";
import { errorMessage, useAsync } from "../../state/useAsync";
import { isFinal, statusLine } from "./messages";
import type { TransferRole } from "./openTransfer";
import { ScanOverlay } from "./ScanOverlay";
import { isIos, scanner } from "./scanner";
import { MethodView, PickView, SessionView, ShowView, TypeView } from "./TransferViews";
import { useLinkSession } from "./useLinkSession";
import "../settings/settings.css";
import "../ai/panes.css";
import "./transfer.css";

type Step = "pick" | "method" | "show" | "type" | "scan";

const LOCAL_NETWORK_KEY = "borealis.localNetworkExplained";
const CANCELLED = "Transfer cancelled. Nothing was changed.";
const CAMERA_OFF = "Camera access is off for Borealis, so type the code instead. You can allow the camera for Borealis in the system Settings.";
const NO_CAMERA = "This device cannot scan a code here. Type the code and address shown under the QR code instead.";

/** Also kept in memory: storage can be unavailable (then the note shows once per app run). */
let explainedThisRun = false;
function localNetworkExplained(): boolean {
  if (explainedThisRun) return true;
  try { return localStorage.getItem(LOCAL_NETWORK_KEY) === "1"; } catch { return false; }
}
function rememberLocalNetworkExplained(): void {
  explainedThisRun = true;
  try { localStorage.setItem(LOCAL_NETWORK_KEY, "1"); } catch { /* shown again after a restart */ }
}

/** Where a role starts: Send picks first; Receive shows the code (desktop) or the connect choices (phones). */
function firstStep(role: TransferRole, mobile: boolean): Step {
  if (role === "send") return "pick";
  return mobile ? "method" : "show";
}

/**
 * "Send to device" / "Receive from device" (spec 4): the device that shows the QR listens, the scanning device
 * connects, and the button pressed decides which way the data goes. Results land in the status bar.
 */
export function TransferDialog({ role, initialIds, onClose }: { role: TransferRole; initialIds: string[]; onClose: () => void }) {
  const api = useApi();
  const { mobile } = usePlatform();
  const { snapshot } = useCharacter();
  const session = useLinkSession(api);
  const s = session.status;
  const [step, setStep] = useState<Step>(firstStep(role, mobile));
  const [chosen, setChosen] = useState<Set<string>>(() => new Set(initialIds.length ? initialIds : snapshot ? [snapshot.id] : []));
  const [packs, setPacks] = useState<LinkPackChoice>("required");
  const [code, setCode] = useState("");
  const [host, setHost] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [explain, setExplain] = useState<(() => void) | null>(null);
  const scanRun = useRef(0);
  const characters = useAsync(() => (role === "send" ? api.listCharacters() : Promise.resolve([])), [api, role]);
  const selection: LinkSelection = { characterIds: [...chosen], packs };
  const title = role === "send" ? "Send to device" : "Receive from device";

  /** iOS asks for Local Network access on the first LAN connection: say so just before it (once). */
  const gated = useCallback((run: () => void) => () => {
    if (isIos() && !localNetworkExplained()) setExplain(() => run);
    else run();
  }, []);

  const call = useCallback(async (work: () => Promise<void>) => {
    setError(null);
    setBusy(true);
    try { await work(); } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  }, []);

  const startShow = gated(() => {
    setStep("show");
    void call(async () => session.adopt(role === "send" ? await api.linkStartSend({ selection }) : await api.linkStartReceive()));
  });
  const connect = (target: string, address?: string) => call(async () => {
    session.adopt(await api.linkConnect({ target, host: address?.trim() || undefined, selection: role === "send" ? selection : undefined }));
  });
  const submitCode = gated(() => void connect(code.trim(), host));
  const startScan = gated(() => {
    const run = ++scanRun.current;
    setNotice(null);
    setError(null);
    setStep("scan");
    void scanner(mobile).scan(role === "receive" ? "send" : "receive").then((r) => {
      if (run !== scanRun.current) return; // the user left the scan screen
      if (r.kind === "ok") { setStep("method"); void connect(r.text); }
      else if (r.kind === "cancelled") setStep("method");
      else { setNotice(r.kind === "denied" ? CAMERA_OFF : NO_CAMERA); setStep("type"); }
    });
  });
  const leaveScan = (next: Step) => {
    scanRun.current += 1;
    void scanner(mobile).cancel();
    setStep(next);
  };

  // Desktop Receive opens straight on the code.
  const opened = useRef(false);
  useEffect(() => {
    if (opened.current) return;
    opened.current = true;
    if (step === "show") startShow();
  }, [step, startShow]);

  // Finished sessions report in the status bar; done / declined / cancelled close the dialog.
  const phase = s?.phase;
  useEffect(() => {
    if (!s || !phase || !isFinal(phase)) return;
    const line = statusLine(s);
    if (line) setStatus(line);
    if (phase === "done" || phase === "declined" || phase === "cancelled") onClose();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- react to the phase only
  }, [phase, s?.sessionId]);

  const close = () => {
    if (step === "scan") leaveScan(step);
    if (s && !isFinal(s.phase)) {
      void api.linkCancel({ sessionId: s.sessionId }).catch(() => undefined);
      setStatus(CANCELLED);
    }
    onClose();
  };
  /** After a failure or expiry: a fresh code on the listening side, otherwise back to the step the user came from. */
  const retry = () => {
    session.reset();
    setError(null);
    if (step === "show") startShow();
    else setStep(step === "scan" ? "method" : step);
  };
  const accept = (yes: boolean) => s && void call(async () => session.adopt(yes ? await api.linkAccept({ sessionId: s.sessionId }) : await api.linkDecline({ sessionId: s.sessionId })));

  const cancelBtn = <Button key="cancel" onClick={close}>Cancel</Button>;
  const canSend = chosen.size > 0 || packs === "all";
  let body: ReactNode;
  let footer: ReactNode[];
  if (s && s.phase === "waiting") {
    body = <ShowView status={s} remaining={session.remaining} role={role} mobile={mobile} />;
    footer = [<Button key="type" onClick={() => { void api.linkCancel({ sessionId: s.sessionId }).catch(() => undefined); session.reset(); setStep("type"); }}>Enter a code instead</Button>, cancelBtn];
  } else if (s) {
    body = <SessionView status={s} />;
    if (s.phase === "preview" && s.role === "receive") {
      footer = [<Button key="accept" variant="highlight" disabled={busy} onClick={() => accept(true)} data-autofocus>Accept</Button>, <Button key="decline" disabled={busy} onClick={() => accept(false)}>Decline</Button>];
    } else if (s.phase === "failed" || s.phase === "expired") {
      footer = [<Button key="retry" variant="highlight" onClick={retry}>{s.phase === "expired" ? "New code" : "Try again"}</Button>, <Button key="close" onClick={onClose}>Close</Button>];
    } else footer = [cancelBtn];
  } else if (step === "pick") {
    body = <PickView characters={characters.data} chosen={chosen} onChosen={setChosen} packs={packs} onPacks={setPacks} unsavedName={snapshot?.dirty && chosen.has(snapshot.id) ? snapshot.name : null} />;
    footer = mobile
      ? [<Button key="next" variant="highlight" disabled={!canSend} onClick={() => setStep("method")}>Next</Button>, cancelBtn]
      : [<Button key="show" variant="highlight" disabled={!canSend || busy} onClick={startShow}>Show code</Button>, <Button key="type" disabled={!canSend} onClick={() => setStep("type")}>Enter a code</Button>, cancelBtn];
  } else if (step === "method") {
    body = <MethodView role={role} onScan={startScan} onShow={startShow} onType={() => { setNotice(null); setStep("type"); }} />;
    footer = role === "send" ? [<Button key="back" onClick={() => setStep("pick")}>Back</Button>, cancelBtn] : [cancelBtn];
  } else if (step === "type" || step === "scan") {
    body = <TypeView code={code} onCode={setCode} host={host} onHost={setHost} needsHost={mobile} notice={notice} error={error} onSubmit={submitCode} />;
    const back: Step | null = mobile ? "method" : role === "send" ? "pick" : null;
    footer = [
      <Button key="connect" variant="highlight" disabled={busy || !code.trim() || (mobile && !host.trim())} onClick={submitCode}>Connect</Button>,
      back ? <Button key="back" onClick={() => { setError(null); setStep(back); }}>Back</Button> : <Button key="show" onClick={() => { setError(null); startShow(); }}>Show a code</Button>,
      cancelBtn,
    ];
  } else if (error) {
    body = null;
    footer = [<Button key="retry" variant="highlight" onClick={startShow}>Try again</Button>, cancelBtn];
  } else {
    body = <p className="xfer-wait" role="status">Opening the connection…</p>;
    footer = [cancelBtn];
  }

  return (
    <>
      <Dialog title={title} onClose={close} className="xfer-dlg" footer={footer}>
        {error && step !== "type" && step !== "scan" && <Alert tone="danger" role="alert">{error}</Alert>}
        {body}
      </Dialog>
      {step === "scan" && <ScanOverlay title="Scan the code" onType={() => leaveScan("type")} onCancel={() => leaveScan("method")} />}
      {explain && (
        <MessageBox title="Local network access" cancelId="ok" buttons={[{ id: "ok", label: "Continue" }]}
          message="Borealis connects to your other device directly over this Wi-Fi. iOS now asks whether Borealis may find devices on your local network: choose Allow, or the two cannot connect. Nothing leaves your network."
          onResult={() => { const run = explain; setExplain(null); rememberLocalNetworkExplained(); run(); }} />
      )}
    </>
  );
}
