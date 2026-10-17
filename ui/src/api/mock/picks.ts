import type { FileDrop, OpenedFile, PickPurpose, PickedPath } from "../types";
import { apiError } from "./errors";

/** Where the mock "native dialog" lands for each purpose. */
const MOCK_PICKS: Record<PickPurpose, string> = {
  "import-file": "/home/me/notes.txt",
  "5etools-folder": "/home/me/5etools/data",
  "content-folder": "/home/me/Documents/Borealis/custom",
  "ai-override": "/usr/local/bin/claude",
  "documents-dir": "/home/me/Documents/Borealis",
  "secondary-content-dir": "/home/me/Documents/Custom Content",
  "character-file": "/home/me/Documents/Mira.dnd5e",
  "dropped-file": "",
};

/** Mirrors `picked_files.rs`: what a drop may carry and what each purpose may read. */
const DROP_EXT = ["xml", "index", "zip", "json", "dnd5e"];
const READABLE: Partial<Record<PickPurpose, string[]>> = { "dropped-file": DROP_EXT, "character-file": ["dnd5e", "json"] };
const ext = (name: string) => name.toLowerCase().split(".").slice(1).pop() ?? "";
const baseName = (path: string) => path.split("/").pop() ?? path;

/** Mirrors the backend's token store: tokens are opaque and only resolve for their purpose. */
export class MockPicks {
  private picks = new Map<string, { path: string; purpose: PickPurpose }>();
  private seq = 0;
  /** Set to `null` to simulate a cancelled dialog. */
  next: string | null | undefined = undefined;

  /** File contents behind mock paths (tests may add); unknown paths read as `{}`. */
  readonly contents = new Map<string, string>();

  pick(purpose: PickPurpose, startDir?: string): PickedPath[] {
    if (purpose === "dropped-file") throw apiError("invalid_argument", "Drop files on the window to use them.");
    const path = this.next === undefined ? (startDir?.trim() || MOCK_PICKS[purpose]) : this.next;
    this.next = undefined;
    if (path === null) return [];
    const token = `pick_mock_${++this.seq}`;
    this.picks.set(token, { path, purpose });
    return [{ token, name: path.split("/").pop() ?? path, path }];
  }

  resolve(token: string, purpose: PickPurpose): string {
    const p = this.picks.get(token);
    if (!p || p.purpose !== purpose) throw apiError("invalid_argument", "That file choice has expired or is not valid here. Choose it again.");
    return p.path;
  }

  /** A native drop: tokens for the allowed files, names of the rest. */
  drop(files: { name: string; contentBase64: string }[]): FileDrop {
    const out: FileDrop = { files: [], rejected: [] };
    for (const f of files) {
      if (!DROP_EXT.includes(ext(f.name))) { out.rejected.push(f.name); continue; }
      const path = `/home/me/Desktop/${f.name}`;
      const token = `pick_mock_${++this.seq}`;
      this.picks.set(token, { path, purpose: "dropped-file" });
      this.contents.set(path, f.contentBase64);
      out.files.push({ token, name: f.name });
    }
    return out;
  }

  /** `stage_5etools_url`: a downloaded data folder becomes a `5etools-folder` token. */
  stageUrl(url: string): PickedPath {
    const token = `pick_mock_${++this.seq}`;
    this.picks.set(token, { path: `/home/me/.download/data`, purpose: "5etools-folder" });
    return { token, name: url, path: url };
  }

  /** `stage_5etools_zip`: a `.zip` becomes a `5etools-folder` token (the mock does not unpack it). */
  stageZip(file: { fileName: string; contentBase64: string }): PickedPath {
    if (ext(file.fileName) !== "zip") throw apiError("invalid_argument", "Choose a .zip of your 5etools data/ folder.");
    if (!file.contentBase64) throw apiError("invalid_argument", "That .zip file is empty or too large to import.");
    const token = `pick_mock_${++this.seq}`;
    this.picks.set(token, { path: `/home/me/.zip/${file.fileName}/data`, purpose: "5etools-folder" });
    return { token, name: file.fileName, path: file.fileName };
  }

  /** `take_picked_files`: all tokens are checked before any is consumed. */
  read(purpose: PickPurpose, tokens: string[]): OpenedFile[] {
    const allowed = READABLE[purpose];
    if (!allowed) throw apiError("invalid_argument", "Those files cannot be read this way.");
    if (tokens.length === 0) throw apiError("invalid_argument", "Choose between 1 and 32 files.");
    const paths = tokens.map((t) => this.resolve(t, purpose));
    const bad = paths.find((p) => !allowed.includes(ext(p)));
    if (bad) throw apiError("invalid_argument", `${baseName(bad)} is not a file Borealis can open here.`);
    tokens.forEach((t) => this.picks.delete(t));
    return paths.map((p) => ({ fileName: baseName(p), contentBase64: this.contents.get(p) ?? btoa("{}") }));
  }

  take(token: string, purpose: PickPurpose): string {
    const path = this.resolve(token, purpose);
    this.picks.delete(token);
    return path;
  }
}
