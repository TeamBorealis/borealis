import type { SearchSettings, SemanticConsent, SemanticDownload, SemanticFile, SemanticLicense, SemanticStatus, SemanticTier } from "../types";
import { apiError } from "./errors";

/* Consent wording, kept equal to `borealis_embed::models::consent_body` by `src-tauri/tests/semantic_model.rs`. */
const CONSENT_TITLE = "Turn on semantic search?";
const CONSENT_ACCEPT = "Download and turn on";
const CONSENT_DECLINE = "Not now";
const SIZE_Q8_0 = "about 310 MB";

const Q8_0: SemanticFile = { quant: "Q8_0", fileName: "embeddinggemma-2-Q8_0.gguf", bytes: 309_855_456, sizeText: SIZE_Q8_0 };
const T1_FILE: SemanticFile = { quant: "Q6_K-te-Q4_K", fileName: "embeddinggemma-2-Q6_K-te-Q4_K.gguf", bytes: 211_161_312, sizeText: "210 MB" };
const NOT_AVAILABLE = "The model for this device is not available yet.";
const NO_BACKEND = "Model downloaded. Semantic search arrives in a later update of Borealis; until then search uses keywords.";
const T0_REASON = "Semantic search needs at least 3 GB of memory. Keyword search works fully.";

/** Spec section 8 consent body for `file` (the "; N on this device" clause only for the smaller file, charging only on mobile). */
export function consentBody(file: SemanticFile, mobile: boolean): string {
  const size = file.quant === Q8_0.quant ? `Apache 2.0, ${SIZE_Q8_0}` : `Apache 2.0, ${SIZE_Q8_0}; ${file.sizeText} on this device`;
  const charging = mobile ? " and runs while your device is charging" : "";
  return `Borealis will download the EmbeddingGemma 2 model (${size}) once, from Hugging Face or the Borealis mirror. `
    + "Search then also finds rules, lore and session notes by meaning, not only by the words you type. "
    + "Everything runs on this device; your searches and notes never leave it. "
    + `Indexing takes a few minutes${charging}. You can delete the model at any time in Settings.`;
}

const consent = (file: SemanticFile, mobile: boolean): SemanticConsent => ({ title: CONSENT_TITLE, body: consentBody(file, mobile), accept: CONSENT_ACCEPT, decline: CONSENT_DECLINE });
const ORDER: SemanticTier[] = ["t0", "t1", "t2", "t3"];

export interface MockSemanticOptions {
  /** Highest tier the mock device allows (default t3 desktop, t2 phone). */
  probedTier: SemanticTier;
  /** An embedding backend is linked (default false, like the app before B-46). */
  backendAvailable: boolean;
  /** The T1 file has a URL (default false: mirror not published, B-46). */
  mirrorPublished: boolean;
  /** The model file is already on the device. */
  modelPresent: boolean;
  /** Delay between simulated progress steps in ms (default 30). */
  stepMs: number;
  /** The next download fails with this message. */
  failWith: string | null;
}

/** Mirrors `semantic_service.rs`: tier, consent text, a stepped download with pause / resume / cancel, delete. */
export class MockSemantic {
  readonly opts: MockSemanticOptions;
  /** Every download start (consent accepted or Resume): the mock's stand-in for network requests. */
  starts = 0;
  private present: boolean;
  private download: SemanticDownload = { state: "idle", done: 0, total: 0, message: null };
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly emit: (d: SemanticDownload) => void, private readonly mobile: boolean, opts: Partial<MockSemanticOptions> = {}) {
    this.opts = { probedTier: mobile ? "t2" : "t3", backendAvailable: false, mirrorPublished: false, modelPresent: false, stepMs: 30, failWith: null, ...opts };
    this.present = this.opts.modelPresent;
  }

  private tier(search: SearchSettings): SemanticTier {
    const probed = ORDER.indexOf(this.opts.probedTier);
    const wanted = search.tierOverride === "auto" ? probed : ORDER.indexOf(search.tierOverride);
    return ORDER[Math.min(probed, wanted)]!;
  }

  private file(tier: SemanticTier): SemanticFile | null {
    return tier === "t0" ? null : tier === "t1" ? T1_FILE : Q8_0;
  }

  status(search: SearchSettings): SemanticStatus {
    const tier = this.tier(search);
    const file = this.file(tier);
    const available = !!file && (file !== T1_FILE || this.opts.mirrorPublished);
    const present = !!file && this.present;
    const reason = !file ? T0_REASON : !available && !present ? NOT_AVAILABLE : null;
    const download = present && this.download.state !== "downloading"
      ? { state: "done" as const, done: file!.bytes, total: file!.bytes, message: this.opts.backendAvailable ? null : NO_BACKEND }
      : { ...this.download };
    return {
      tier, probedTier: this.opts.probedTier, mobile: this.mobile, file, available, unavailableReason: reason, modelPresent: present,
      backendAvailable: this.opts.backendAvailable, semantic: search.semantic, active: search.semantic && present && this.opts.backendAvailable,
      consent: file ? consent(file, this.mobile) : null, download,
    };
  }

  start(search: SearchSettings): void {
    const st = this.status(search);
    if (!st.file) throw apiError("requirement_failed", "Semantic search is not available on this device.");
    if (!st.available && !st.modelPresent) throw apiError("requirement_failed", NOT_AVAILABLE);
    if (st.modelPresent || this.download.state === "downloading") return;
    this.starts += 1;
    const total = st.file.bytes;
    this.set({ state: "downloading", done: this.download.state === "paused" ? this.download.done : 0, total, message: null });
    this.step(total);
  }

  private step(total: number) {
    this.timer = setTimeout(() => {
      if (this.download.state !== "downloading") return;
      if (this.opts.failWith) {
        this.set({ ...this.download, state: "failed", message: this.opts.failWith });
        return;
      }
      const done = Math.min(total, this.download.done + Math.ceil(total / 4));
      if (done < total) {
        this.set({ ...this.download, done });
        this.step(total);
        return;
      }
      this.present = true;
      this.set({ state: "done", done: total, total, message: this.opts.backendAvailable ? null : NO_BACKEND });
    }, this.opts.stepMs);
  }

  pause(): void {
    if (this.download.state !== "downloading") return;
    this.clear();
    this.set({ ...this.download, state: "paused" });
  }

  cancel(): void {
    this.clear();
    this.set({ state: "idle", done: 0, total: 0, message: null });
  }

  delete(): void {
    this.cancel();
    this.present = false;
  }

  license(): SemanticLicense {
    return { model: "EmbeddingGemma 2", license: "Apache License\nVersion 2.0, January 2004\n(mock text)\nEND OF TERMS AND CONDITIONS", notice: "EmbeddingGemma 2 model weights\nCopyright Google LLC / Google DeepMind. (mock NOTICE)" };
  }

  private clear() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  private set(d: SemanticDownload) {
    this.download = d;
    this.emit({ ...d });
  }
}
