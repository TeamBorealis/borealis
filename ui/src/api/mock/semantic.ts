import type { SearchSettings, SemanticConsent, SemanticDownload, SemanticFile, SemanticIndexing, SemanticLicense, SemanticStatus, SemanticTier } from "../types";
import { apiError } from "./errors";

/* Consent wording, kept equal to `borealis_embed::models::consent_body` by `src-tauri/tests/semantic_model.rs`. */
const CONSENT_TITLE = "Turn on semantic search?";
const CONSENT_ACCEPT = "Download and turn on";
const CONSENT_DECLINE = "Not now";
const SIZE_Q8_0 = "about 310 MB";
const INDEXING_DESKTOP = "Indexing takes a few minutes.";
const INDEXING_X86 = "Indexing takes a few minutes. On slower computers, search may stay keyword-only after the download. Borealis checks the speed once the model has loaded.";
const INDEXING_PHONE = "Indexing can take several hours and runs while your device is charging. On slower phones, search may stay keyword-only after the download. Borealis checks the speed once the model has loaded.";

const Q8_0: SemanticFile = { quant: "Q8_0", fileName: "embeddinggemma-2-Q8_0.gguf", bytes: 309_855_456, sizeText: SIZE_Q8_0 };
const T1_FILE: SemanticFile = { quant: "Q6_K-te-Q4_K", fileName: "embeddinggemma-2-Q6_K-te-Q4_K.gguf", bytes: 211_161_312, sizeText: "210 MB" };
const NOT_AVAILABLE = "The model for this device is not available yet.";
const NO_BACKEND = "Model downloaded. Semantic search arrives in a later update of Borealis; until then search uses keywords.";
const NO_BACKEND_YET = "Semantic search (search by meaning) arrives in a later update. Search uses keywords until then.";
const T0_REASON = "Semantic search needs at least 3 GB of memory. Keyword search works fully.";
const INDEX_OFF: SemanticIndexing = { state: "off", done: 0, total: 0, message: null, queryable: false };
/** Passages the simulated index embeds (in four steps). */
const INDEX_TOTAL = 1200;

/** Which indexing sentences the consent carries (`borealis_embed::models::ConsentDevice`): x86_64 desktops (B-49) and
 * phones (PH1) are told search may stay keyword-only after the download. */
export type ConsentDevice = "desktop" | "x86Desktop" | "phone";
const INDEXING: Record<ConsentDevice, string> = { desktop: INDEXING_DESKTOP, x86Desktop: INDEXING_X86, phone: INDEXING_PHONE };

/** Spec section 8 consent body for `file` (the "; N on this device" clause only for the smaller file; the indexing sentences per device kind). */
export function consentBody(file: SemanticFile, device: ConsentDevice): string {
  const size = file.quant === Q8_0.quant ? `Apache 2.0, ${SIZE_Q8_0}` : `Apache 2.0, ${SIZE_Q8_0}; ${file.sizeText} on this device`;
  const indexing = INDEXING[device];
  return `Borealis will download the EmbeddingGemma 2 model (${size}) once, from Hugging Face or the Borealis mirror. `
    + "Search then also finds rules, lore and session notes by meaning, not only by the words you type. "
    + "Everything runs on this device; your searches and notes never leave it. "
    + `${indexing} You can delete the model at any time in Settings.`;
}

const consent = (file: SemanticFile, device: ConsentDevice): SemanticConsent => ({ title: CONSENT_TITLE, body: consentBody(file, device), accept: CONSENT_ACCEPT, decline: CONSENT_DECLINE });
const ORDER: SemanticTier[] = ["t0", "t1", "t2", "t3"];

export interface MockSemanticOptions {
  /** Highest tier the mock device allows (default t3 desktop, t2 phone). */
  probedTier: SemanticTier;
  /** A desktop with an x86_64 processor (Windows, Linux; default false): the consent says slower computers may stay
   * keyword-only (B-49). Ignored on phones. */
  x86: boolean;
  /** An embedding backend is linked (default true, like the app since B-12; false = a build without it). */
  backendAvailable: boolean;
  /** The T1 file has a URL (default true: the mirror is published, B-46). */
  mirrorPublished: boolean;
  /** The model file is already on the device. */
  modelPresent: boolean;
  /** The semantic index is already built when semantic search is on with the model present (default false: it
   * builds in four `stepMs` steps first, like `vector_service.rs`). */
  indexed: boolean;
  /** Delay between simulated progress steps in ms (default 30). */
  stepMs: number;
  /** The next download fails with this message. */
  failWith: string | null;
  /** The device's query embedding time in ms (the app's speed check after loading the model, `vector_service.rs`):
   * above `QUERY_BUDGET_MS` the index is not built and search stays keyword-only with the too-slow line
   * (default null: fast enough). */
  queryMs: number | null;
}

/** The hybrid latency budget every semantic tier carries (`borealis_embed::tier::QUERY_BUDGET_MS`). */
export const QUERY_BUDGET_MS = 40;

/** `vector_service.rs` too-slow line (decision PH1: the same sentence on desktops and phones). */
export function tooSlowMessage(ms: number): string {
  return `Semantic search is too slow on this device: one search takes ${ms} ms to embed (the budget is ${QUERY_BUDGET_MS} ms), so search uses keywords. You can delete the model in Settings.`;
}

/** Mirrors `semantic_service.rs` and `vector_service.rs`: tier, consent text, a stepped download with pause / resume /
 * cancel, delete, and a stepped semantic index (`active` only once it is built). */
export class MockSemantic {
  readonly opts: MockSemanticOptions;
  /** Every download start (consent accepted or Resume): the mock's stand-in for network requests. */
  starts = 0;
  private present: boolean;
  private download: SemanticDownload = { state: "idle", done: 0, total: 0, message: null };
  private timer: ReturnType<typeof setTimeout> | null = null;
  private indexing: SemanticIndexing = { ...INDEX_OFF };
  private indexTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly emit: (d: SemanticDownload) => void, private readonly mobile: boolean, opts: Partial<MockSemanticOptions> = {},
    private readonly emitIndex: (i: SemanticIndexing) => void = () => {}) {
    this.opts = { probedTier: mobile ? "t2" : "t3", x86: false, backendAvailable: true, mirrorPublished: true, modelPresent: false, indexed: false, stepMs: 30, failWith: null, queryMs: null, ...opts };
    this.present = this.opts.modelPresent;
  }

  /** Start, keep or drop the simulated index for the current settings (what the app's indexer thread does). */
  private syncIndex(search: SearchSettings, present: boolean) {
    const wanted = search.semantic && present && this.opts.backendAvailable && this.tier(search) !== "t0";
    if (!wanted) {
      if (this.indexTimer) clearTimeout(this.indexTimer);
      this.indexTimer = null;
      if (this.indexing.state !== "off") this.setIndex({ ...INDEX_OFF });
      return;
    }
    if (this.indexing.state !== "off") return;
    const slow = this.opts.queryMs;
    if (slow !== null && slow > QUERY_BUDGET_MS) {
      // Loads the model, times a query, unloads it: nothing is indexed and search stays keyword-only.
      this.setIndex({ state: "loading", done: 0, total: 0, message: null, queryable: false });
      this.indexTimer = setTimeout(() => {
        this.indexTimer = null;
        this.setIndex({ state: "failed", done: 0, total: 0, message: tooSlowMessage(slow), queryable: false });
      }, this.opts.stepMs);
      return;
    }
    if (this.opts.indexed) {
      this.setIndex({ state: "ready", done: 0, total: 0, message: null, queryable: true });
      return;
    }
    this.setIndex({ state: "indexing", done: 0, total: INDEX_TOTAL, message: null, queryable: false });
    const step = () => {
      this.indexTimer = setTimeout(() => {
        const done = Math.min(INDEX_TOTAL, this.indexing.done + INDEX_TOTAL / 4);
        if (done < INDEX_TOTAL) {
          this.setIndex({ ...this.indexing, done });
          step();
          return;
        }
        this.indexTimer = null;
        this.setIndex({ state: "ready", done, total: INDEX_TOTAL, message: null, queryable: true });
      }, this.opts.stepMs);
    };
    step();
  }

  private setIndex(i: SemanticIndexing) {
    this.indexing = i;
    this.emitIndex({ ...i });
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
    this.syncIndex(search, present);
    const active = search.semantic && present && this.opts.backendAvailable && this.indexing.queryable;
    return {
      tier, probedTier: this.opts.probedTier, mobile: this.mobile, file, available, unavailableReason: reason, modelPresent: present,
      backendAvailable: this.opts.backendAvailable, semantic: search.semantic, active,
      consent: file ? consent(file, this.mobile ? "phone" : this.opts.x86 ? "x86Desktop" : "desktop") : null, download, indexing: { ...this.indexing, queryable: active },
    };
  }

  start(search: SearchSettings): void {
    const st = this.status(search);
    if (!this.opts.backendAvailable) throw apiError("requirement_failed", NO_BACKEND_YET);
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
