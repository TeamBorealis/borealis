import type { LinkDevice, LinkPhase, LinkPlatform, LinkPreview, LinkSelection, LinkStatus } from "../types";
import { apiError } from "./errors";

const CODE = /^\s*\d{1,2}[\s-]+[a-z]+[\s-]+[a-z]+\s*$/i;
const QR_PREFIX = "borealis-link:v1?";
const DAY_SECS = 86_400;
const PAIRING_TTL_DAYS = 180;
const FINAL: readonly LinkPhase[] = ["done", "failed", "cancelled", "declined", "expired"];

const blank = (sessionId: string, role: "send" | "receive", phase: LinkPhase): LinkStatus => ({
  sessionId, role, phase, qrPayload: null, code: null, address: null, fingerprint: null, expiresInSecs: null,
  peerName: null, preview: null, bytesDone: null, bytesTotal: null, summary: null, errorCode: null, error: null,
});

/** What a peer offers in the mock: one row per preview action the real planner produces. */
const MOCK_PREVIEW: LinkPreview = {
  totalBytes: 96_420,
  items: [
    { kind: "character", key: "c_mock", label: "Mira", action: "new", size: 48_000, missingPacks: [] },
    { kind: "character", key: "c_mock2", label: "Tobin Ashgrove", action: "replace", size: 41_220, missingPacks: [] },
    { kind: "pack", key: "tavern-feats", label: "Tavern Feats (homebrew)", action: "keep_local", size: 7_200, missingPacks: [] },
  ],
};

export interface MockLinkOptions {
  /** This device is a phone (names in the QR and the peer differ). */
  mobile?: boolean;
  /** A simulated peer connects to an open window after this many ms (null = only through `simulatePeer`). */
  peerAfterMs?: number | null;
  /** Paired devices to start with. */
  devices?: LinkDevice[];
  /** Clock in unix seconds (tests). */
  now?: () => number;
  /** A receive finished: the names of the characters that are new here (the mock library adds them). */
  onReceived?: (newCharacters: string[]) => void;
}

/** Mirrors the Rust device transfer: windows with a QR + code, a preview to accept, progress, a summary, paired devices. */
export class MockDeviceLink {
  private sessions = new Map<string, LinkStatus>();
  private seq = 0;
  private devices: LinkDevice[];
  private readonly selfName: string;
  private readonly peer: { name: string; platform: LinkPlatform };
  private readonly peerAfterMs: number | null;
  private readonly now: () => number;
  private readonly onReceived: (newCharacters: string[]) => void;

  constructor(private readonly emit: (s: LinkStatus) => void, opts: MockLinkOptions = {}) {
    this.selfName = opts.mobile ? "Borealis on iPhone or iPad" : "Borealis on Mac";
    this.peer = opts.mobile ? { name: "Borealis on Mac", platform: "macos" } : { name: "Borealis on iPhone or iPad", platform: "ios" };
    this.peerAfterMs = opts.peerAfterMs ?? null;
    this.devices = [...(opts.devices ?? [])];
    this.now = opts.now ?? (() => Math.floor(Date.now() / 1000));
    this.onReceived = opts.onReceived ?? (() => undefined);
  }

  private put(s: LinkStatus): LinkStatus {
    this.sessions.set(s.sessionId, s);
    this.emit(s);
    return s;
  }

  private id(): string {
    this.seq += 1;
    return `s_mock${this.seq}`;
  }

  private later(sessionId: string): void {
    if (this.peerAfterMs === null) return;
    setTimeout(() => { if (!this.isFinal(sessionId)) this.simulatePeer(sessionId); }, this.peerAfterMs);
  }

  private isFinal(sessionId: string): boolean {
    const s = this.sessions.get(sessionId);
    return !s || FINAL.includes(s.phase);
  }

  private window(role: "send" | "receive"): LinkStatus {
    const name = encodeURIComponent(this.selfName);
    const s = this.put({
      ...blank(this.id(), role, "waiting"),
      qrPayload: `${QR_PREFIX}h=192.168.1.20:41234&fp=q83vEjRWeJCrze8SNFZ4kKvN7xI0VniQq83vEjRWeJA&c=AAECAwQFBgcICQoLDA0ODw&n=${name}&m=${role}`,
      code: "42-ember-lantern",
      address: "192.168.1.20:41234",
      fingerprint: "ab12 cd34 ef56 7890",
      expiresInSecs: 300,
    });
    this.later(s.sessionId);
    return s;
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
    if (selection && !selection.characterIds.length && selection.packs !== "all") throw apiError("invalid_argument", "Choose at least one character to send.");
    const s = this.put({ ...blank(this.id(), role, "connecting"), peerName: this.peer.name });
    this.remember();
    if (role === "send") {
      const waiting = this.put({ ...s, phase: "waiting_for_peer" });
      this.later(waiting.sessionId);
      return waiting;
    }
    return this.put({ ...s, phase: "preview", bytesTotal: MOCK_PREVIEW.totalBytes, preview: MOCK_PREVIEW });
  }

  /** The simulated other device takes its next step: connects to an open window, or accepts what this device sends. */
  simulatePeer(sessionId: string): LinkStatus {
    const s = this.status(sessionId);
    if (s.phase === "waiting" && s.role === "receive") {
      this.put({ ...s, phase: "connecting", qrPayload: null, code: null, peerName: this.peer.name });
      this.remember();
      return this.put({ ...s, phase: "preview", qrPayload: null, code: null, peerName: this.peer.name, bytesTotal: MOCK_PREVIEW.totalBytes, preview: MOCK_PREVIEW });
    }
    if (s.phase === "waiting" && s.role === "send") {
      this.remember();
      const waiting = this.put({ ...s, phase: "waiting_for_peer", qrPayload: null, code: null, peerName: this.peer.name });
      this.later(sessionId);
      return waiting;
    }
    if (s.phase === "waiting_for_peer") return this.finish(s);
    return s;
  }

  private finish(s: LinkStatus): LinkStatus {
    const total = s.bytesTotal ?? MOCK_PREVIEW.totalBytes;
    this.put({ ...s, phase: "transferring", bytesDone: Math.round(total / 2), bytesTotal: total });
    if (s.role === "receive") this.onReceived((s.preview?.items ?? []).filter((i) => i.kind === "character" && i.action === "new").map((i) => i.label));
    return this.put({ ...s, phase: "done", bytesDone: total, bytesTotal: total, summary: { added: 1, replaced: 1, keptLocal: 1, identical: 0 } });
  }

  /** Pairing memory, as the Rust side keeps it after the peer's hello. */
  private remember(): void {
    const now = this.now();
    const id = this.peer.platform === "ios" ? "a1b2c3d4e5f60718" : "0f1e2d3c4b5a6978";
    const known = this.devices.find((d) => d.id === id);
    const entry: LinkDevice = { id, name: this.peer.name, platform: this.peer.platform, pairedAt: known?.pairedAt ?? now, lastSyncedAt: now, expiresAt: now + PAIRING_TTL_DAYS * DAY_SECS };
    this.devices = [entry, ...this.devices.filter((d) => d.id !== id)];
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
    return this.finish(s);
  }

  cancel(sessionId: string): LinkStatus {
    const s = this.status(sessionId);
    return FINAL.includes(s.phase) ? s : this.put({ ...s, phase: "cancelled", qrPayload: null, code: null });
  }

  listDevices(): LinkDevice[] {
    const now = this.now();
    this.devices = this.devices.filter((d) => d.expiresAt > now);
    return [...this.devices].sort((a, b) => b.lastSyncedAt - a.lastSyncedAt || a.name.localeCompare(b.name));
  }

  forgetDevice(deviceId: string): LinkDevice[] {
    if (!/^[0-9a-f]{16}$/.test(deviceId)) throw apiError("invalid_argument", "Unknown device.");
    this.devices = this.devices.filter((d) => d.id !== deviceId);
    return this.listDevices();
  }
}
