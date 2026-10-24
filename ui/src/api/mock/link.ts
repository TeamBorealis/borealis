import type { LinkPhase, LinkSelection, LinkStatus } from "../types";
import { apiError } from "./errors";

const CODE = /^\s*\d{1,2}[\s-]+[a-z]+[\s-]+[a-z]+\s*$/i;
const QR_PREFIX = "borealis-link:v1?";

const blank = (sessionId: string, role: "send" | "receive", phase: LinkPhase): LinkStatus => ({
  sessionId, role, phase, qrPayload: null, code: null, address: null, fingerprint: null, expiresInSecs: null,
  peerName: null, preview: null, bytesDone: null, bytesTotal: null, summary: null, errorCode: null, error: null,
});

/** Mirrors the Rust device transfer: windows with a QR + code, a preview to accept, progress, a summary. */
export class MockDeviceLink {
  private sessions = new Map<string, LinkStatus>();
  private seq = 0;

  constructor(private readonly emit: (s: LinkStatus) => void) {}

  private put(s: LinkStatus): LinkStatus {
    this.sessions.set(s.sessionId, s);
    this.emit(s);
    return s;
  }

  private id(): string {
    this.seq += 1;
    return `s_mock${this.seq}`;
  }

  private window(role: "send" | "receive"): LinkStatus {
    const mode = role === "send" ? "send" : "receive";
    return this.put({
      ...blank(this.id(), role, "waiting"),
      qrPayload: `${QR_PREFIX}h=192.168.1.20:41234&fp=q83vEjRWeJCrze8SNFZ4kKvN7xI0VniQq83vEjRWeJA&c=AAECAwQFBgcICQoLDA0ODw&n=Borealis%20on%20Mac&m=${mode}`,
      code: "42-ember-lantern",
      address: "192.168.1.20:41234",
      fingerprint: "ab12 cd34 ef56 7890",
      expiresInSecs: 300,
    });
  }

  startSend(selection: LinkSelection): LinkStatus {
    if (!selection.characterIds.length && selection.packs !== "all") throw apiError("invalid_argument", "Choose at least one character to send.");
    return this.window("send");
  }

  startReceive(): LinkStatus {
    return this.window("receive");
  }

  connect(target: string, selection?: LinkSelection): LinkStatus {
    const isQr = target.trim().startsWith(QR_PREFIX);
    if (!isQr && !CODE.test(target)) throw apiError("invalid_argument", "That code is not valid. It looks like 42-ember-lantern.");
    const role = selection ? "send" : "receive";
    if (isQr && target.includes(`&m=${role}`)) {
      throw apiError("invalid_argument", role === "send"
        ? "That device is sending, not receiving. Choose Receive here instead."
        : "That device is waiting to receive. Choose Send here and pick what to send.");
    }
    const s = this.put({ ...blank(this.id(), role, "connecting"), peerName: "Borealis on Mac" });
    if (role === "send") return this.put({ ...s, phase: "waiting_for_peer" });
    return this.put({
      ...s,
      phase: "preview",
      bytesTotal: 48_213,
      preview: { totalBytes: 48_213, items: [{ kind: "character", key: "c_mock", label: "Mira", action: "new", size: 48_000, missingPacks: [] }] },
    });
  }

  status(sessionId: string): LinkStatus {
    const s = this.sessions.get(sessionId);
    if (!s) throw apiError("not_found", "No such transfer.");
    return s;
  }

  decide(sessionId: string, accept: boolean): LinkStatus {
    const s = this.status(sessionId);
    if (s.phase !== "preview") throw apiError("invalid_argument", "Nothing is waiting for a decision.");
    if (!accept) return this.put({ ...s, phase: "declined" });
    const total = s.bytesTotal ?? 0;
    this.put({ ...s, phase: "transferring", bytesDone: total });
    return this.put({ ...s, phase: "done", bytesDone: total, summary: { added: 1, replaced: 0, keptLocal: 0, identical: 0 } });
  }

  cancel(sessionId: string): LinkStatus {
    const s = this.status(sessionId);
    const final = ["done", "failed", "cancelled", "declined", "expired"].includes(s.phase);
    return final ? s : this.put({ ...s, phase: "cancelled", qrPayload: null, code: null });
  }
}
