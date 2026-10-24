import type { BundleExport, BundlePreview, BundleShare, LinkPreview, LinkSelection, LinkSummary, PickedPath } from "../types";
import { apiError } from "./errors";

/** What a bundle file holds in the mock: one row per preview action the real planner produces. */
const MOCK_FILE_PREVIEW: LinkPreview = {
  totalBytes: 96_420,
  items: [
    { kind: "character", key: "c_mock", label: "Mira", action: "new", size: 48_000, missingPacks: [] },
    { kind: "character", key: "c_mock2", label: "Tobin Ashgrove", action: "replace", size: 41_220, missingPacks: [] },
    { kind: "pack", key: "tavern-feats", label: "Tavern Feats (homebrew)", action: "keep_local", size: 7_200, missingPacks: [] },
  ],
};

const SUMMARY: LinkSummary = { added: 1, replaced: 1, keptLocal: 1, identical: 0 };
/** Same words as the Rust side (`device_sync/bundle.rs`). */
const DAMAGED = "This bundle cannot be used: it is damaged or was changed after it was made. Nothing was changed.";
const NOT_A_BUNDLE = "This bundle cannot be used: it is not a Borealis bundle. Nothing was changed.";

const baseName = (path: string) => path.split("/").pop() ?? path;

/** Mirrors the Rust bundle files: export, share, open (preview), accept. File names steer the failures: `tampered`, `notabundle`. */
export class MockBundles {
  private open = new Map<string, { fileName: string }>();
  private seq = 0;
  private opened: string[] = [];
  /** What the mock "save dialog" and share sheet received, newest last (tests read these). */
  readonly exported: BundleExport[] = [];
  readonly shared: BundleShare[] = [];
  /** Set to `null` to simulate a cancelled save dialog. */
  nextSavePath: string | null | undefined = undefined;
  /** Phones: Android opens the share sheet from the backend; elsewhere the bytes come back. */
  androidShare = false;

  constructor(
    private readonly resolve: (token: string) => string,
    private readonly mint: (path: string) => PickedPath,
    private readonly onReceived: (newCharacters: string[]) => void,
  ) {}

  private describe(selection: LinkSelection, names: string[]): { fileName: string; characters: number; packs: number; bytes: number } {
    if (!selection.characterIds.length && selection.packs !== "all") throw apiError("invalid_argument", "Choose at least one character to send.");
    const characters = selection.characterIds.length;
    const one = characters === 1 ? names[0] : undefined;
    const fileName = `${(one ?? "Borealis characters").replace(/[^A-Za-z0-9 _-]/g, "_").trim()}.borealis`;
    return { fileName, characters, packs: selection.packs === "none" ? 0 : selection.packs === "all" ? 2 : 0, bytes: 12_000 + 36_000 * characters };
  }

  export(selection: LinkSelection, names: string[]): BundleExport {
    const d = this.describe(selection, names);
    const chosen = this.nextSavePath === undefined ? `/home/me/Documents/${d.fileName}` : this.nextSavePath;
    this.nextSavePath = undefined;
    const out: BundleExport = { ...d, path: chosen, fileName: chosen ? baseName(chosen) : d.fileName };
    this.exported.push(out);
    return out;
  }

  share(selection: LinkSelection, names: string[]): BundleShare {
    const d = this.describe(selection, names);
    const out: BundleShare = { ...d, shared: this.androidShare, dataBase64: this.androidShare ? null : btoa("PK mock bundle") };
    this.shared.push(out);
    return out;
  }

  /** The OS opened a `.borealis` file with the app; `takeOpenedBundles` returns its token. */
  queueOpened(path: string): void {
    this.opened.push(path);
  }

  takeOpened(): PickedPath[] {
    const paths = this.opened;
    this.opened = [];
    return paths.map((p) => this.mint(p));
  }

  openFile(token: string): BundlePreview {
    const path = this.resolve(token);
    const fileName = baseName(path);
    const lower = fileName.toLowerCase();
    if (lower.includes("tampered")) throw apiError("invalid_argument", DAMAGED);
    if (lower.includes("notabundle")) throw apiError("invalid_argument", NOT_A_BUNDLE);
    const bundleId = `b_mock${++this.seq}`;
    this.open.set(bundleId, { fileName });
    return { bundleId, fileName, sourceDevice: "Borealis on Mac", preview: MOCK_FILE_PREVIEW };
  }

  accept(bundleId: string): LinkSummary {
    if (!this.open.delete(bundleId)) throw apiError("not_found", "That bundle is no longer open.");
    this.onReceived(MOCK_FILE_PREVIEW.items.filter((i) => i.kind === "character" && i.action === "new").map((i) => i.label));
    return SUMMARY;
  }

  discard(bundleId: string): void {
    this.open.delete(bundleId);
  }
}
