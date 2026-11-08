import type {
  AppSettings, VaultAttachment, VaultBacklink, VaultChange, VaultConfig, VaultDeleteStatus, VaultEntry, VaultNote, VaultRenameDone,
  VaultRenamePlan, VaultSave, VaultSearchHit, VaultTag, VaultTree,
} from "../types";
import { apiError } from "./errors";
import type { MockPicks } from "./picks";

/** A file of the mock vault: text, or an image as a `data:` URL. */
interface MockFile { text: string; mtimeMs: number }

const DEFAULT_CONFIG: VaultConfig = {
  newFileLocation: "root", newFileFolderPath: "", attachmentFolderPath: "/", useMarkdownLinks: false, newLinkFormat: "shortest",
};
const PNG_URL = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
/** A small vault for the browser build (original text). Tests load the fixture vault instead (`load`). */
const SAMPLE: Record<string, string> = {
  "Home.md": "---\ntags: [campaign]\n---\n# Campaign notes\n\nLast session: [[Session 1]]. Our guide is [[Bramblewick]].\n\n- [ ] Write up the harbor\n",
  "Session 1.md": "# Session 1\n\nWe reached [[Harbor Town]] at dusk. #session\n",
  "People/Bramblewick.md": "# Bramblewick\n\nA gnome tinker. #npc\n\n## Appearance\n\nCopper goggles.\n",
  "Places/Harbor Town.md": "# Harbor Town\n\nA fishing port. ![[map.png]]\n",
  "map.png": PNG_URL,
};
const WIKILINK = /(!?)\[\[([^[\]\r\n]+)\]\]/g;
const TAG = /(?:^|\s)#([\p{L}\p{N}_/-]*[\p{L}_/-][\p{L}\p{N}_/-]*)/gmu;
const IMAGE = /\.(png|jpe?g|gif|webp|bmp|svg|avif)$/i;

const fileName = (p: string) => p.split("/").pop() ?? p;
const parentOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);
const stripMd = (p: string) => p.replace(/\.md$/i, "");
const kindOf = (p: string): VaultEntry["kind"] => (/\.md$/i.test(p) ? "note" : /\.canvas$/i.test(p) ? "canvas" : IMAGE.test(p) ? "image" : "file");
/** Not SHA-256, but a content hash all the same (the mock only compares them). */
function hashOf(text: string): string {
  let h = 5381;
  for (let i = 0; i < text.length; i += 1) h = ((h * 33) ^ text.charCodeAt(i)) >>> 0;
  return `mock-${h.toString(16)}-${text.length}`;
}
function checkPath(path: string): void {
  const parts = path.split("/");
  if (!path || parts.some((p) => !p || p === "." || p === ".." || p.startsWith(".") || /[\\:\0]/.test(p))) throw apiError("invalid_argument", "That file is not in your vault.");
}
function checkName(name: string): void {
  if (!name.trim()) throw apiError("invalid_argument", "Type a name.");
  if (/[/\\:*?"<>|#^[\]]/.test(name)) throw apiError("invalid_argument", "A name cannot contain any of these characters: / \\ : * ? \" < > | # ^ [ ]");
}

/** Mirrors `src-tauri/src/vault` (GitHub #15) closely enough for the notes view: token-only folder, safe writes, links. */
export class MockVault {
  files = new Map<string, MockFile>();
  config: VaultConfig = { ...DEFAULT_CONFIG };
  /** Simulate an unplugged vault folder. */
  unavailable = false;
  /** Simulate a system without a trash: delete answers `trash-unavailable` unless `permanent`. */
  trashAvailable = true;
  /** Paths moved to the mock trash. */
  readonly trashed: string[] = [];
  watching = false;
  readonly opened: string[] = [];
  /** iOS: files iCloud Drive has not downloaded yet (listed as `cloud`, opening one says it is downloading). */
  readonly cloud = new Set<string>();
  private clock = 1_700_000_000_000;

  constructor(private readonly picks: MockPicks, private readonly emit: (c: VaultChange) => void) {
    this.load(SAMPLE);
  }

  /** Replace the vault's files (tests: the fixture vault). */
  load(files: Record<string, string>): void {
    this.files = new Map(Object.entries(files).map(([p, text]) => [p, { text, mtimeMs: this.tick() }]));
  }

  private tick(): number { this.clock += 1000; return this.clock; }

  /** Another app writes (or with `null` deletes) a file: the change is applied and reported like the watcher does. */
  externalWrite(path: string, text: string | null): void {
    if (text === null) this.files.delete(path);
    else this.files.set(path, { text, mtimeMs: this.tick() });
    if (this.watching) this.emit({ paths: [path], all: false, config: false });
  }

  set(s: AppSettings, token?: string): AppSettings {
    return { ...s, vaultDir: token === undefined ? null : this.picks.take(token, "vault-dir") };
  }

  isUnavailable(s: AppSettings): boolean { return s.vaultDir !== null && this.unavailable; }

  private ready(s: AppSettings): void {
    if (s.vaultDir === null) throw apiError("invalid_argument", "No vault is open. Choose your vault folder in Settings > Content.");
    if (this.unavailable) throw apiError("invalid_argument", "Your vault folder is unavailable. Connect it, or choose it again in Settings > Content.");
  }

  private file(path: string): MockFile {
    checkPath(path);
    const f = this.files.get(path);
    if (!f) throw apiError("not_found", "That file is no longer in your vault.");
    return f;
  }

  tree(s: AppSettings): VaultTree {
    const dir = s.vaultDir;
    const name = dir ? fileName(dir) : null;
    if (dir === null || this.unavailable) return { dir, name, unavailable: this.isUnavailable(s), obsidian: false, config: DEFAULT_CONFIG, entries: [], truncated: false };
    const folders = new Set<string>();
    for (const p of this.files.keys()) for (let d = parentOf(p); d; d = parentOf(d)) folders.add(d);
    const entries: VaultEntry[] = [];
    const walk = (folder: string) => {
      const childFolders = [...folders].filter((f) => parentOf(f) === folder).sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
      const childFiles = [...this.files.keys()].filter((f) => parentOf(f) === folder).sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
      for (const f of childFolders) { entries.push({ path: f, name: fileName(f), kind: "folder", size: 0, mtimeMs: 0 }); walk(f); }
      for (const f of childFiles) {
        const file = this.files.get(f)!;
        entries.push(this.cloud.has(f)
          ? { path: f, name: fileName(f), kind: kindOf(f), size: 0, mtimeMs: file.mtimeMs, cloud: true }
          : { path: f, name: fileName(f), kind: kindOf(f), size: file.text.length, mtimeMs: file.mtimeMs });
      }
    };
    walk("");
    return { dir, name, unavailable: false, obsidian: true, config: this.config, entries, truncated: false };
  }

  read(s: AppSettings, path: string): VaultNote {
    this.ready(s);
    const f = this.file(path);
    if (this.cloud.has(path)) {
      throw { ...apiError("io_error", "This file is still downloading from iCloud. It opens when it is ready."), details: { downloading: true } };
    }
    if (!["note", "canvas"].includes(kindOf(path))) throw apiError("invalid_argument", "Borealis opens only notes and canvases as text.");
    this.opened.push(path);
    return this.note(path, f);
  }

  private note(path: string, f: MockFile): VaultNote {
    const kind = kindOf(path);
    return {
      path, text: f.text, hash: hashOf(f.text), mtimeMs: f.mtimeMs, lineEnding: f.text.includes("\r\n") ? "crlf" : "lf", bom: false,
      readOnly: kind === "note" ? null : "Borealis shows this file as plain text and does not change it.",
    };
  }

  save(s: AppSettings, path: string, text: string, baseHash?: string): VaultSave {
    this.ready(s);
    checkPath(path);
    if (kindOf(path) !== "note") throw apiError("invalid_argument", "Only Markdown notes can be edited.");
    const current = this.files.get(path);
    if (baseHash === undefined) {
      if (current) throw apiError("invalid_argument", "A file with that name is already there.");
    } else if (!current) {
      return { status: "missing", hash: null, mtimeMs: null, disk: null };
    } else if (hashOf(current.text) !== baseHash) {
      return { status: "conflict", hash: null, mtimeMs: null, disk: this.note(path, current) };
    }
    const crlf = current?.text.includes("\r\n") ?? false;
    const body = crlf ? text.replace(/\r\n/g, "\n").replace(/\n/g, "\r\n") : text;
    const mtimeMs = this.tick();
    this.files.set(path, { text: body, mtimeMs });
    return { status: "saved", hash: hashOf(body), mtimeMs, disk: null };
  }

  private newFolder(from?: string): string {
    if (this.config.newFileLocation === "current") return from ? parentOf(from) : "";
    if (this.config.newFileLocation === "folder") return this.config.newFileFolderPath;
    return "";
  }

  private free(folder: string, stem: string, ext: string): string {
    for (let n = 0; ; n += 1) {
      const p = join(folder, `${n === 0 ? stem : `${stem} ${n}`}${ext}`);
      if (!this.files.has(p)) return p;
    }
  }

  create(s: AppSettings, name?: string, from?: string, folder?: string): VaultNote {
    this.ready(s);
    let path: string;
    if (name?.trim()) {
      const n = stripMd(name.trim().replace(/^\//, ""));
      n.split("/").forEach(checkName);
      path = n.includes("/") ? `${n}.md` : join(folder ?? this.newFolder(from), `${n}.md`);
      if (this.files.has(path)) throw apiError("invalid_argument", "A note with that name is already there.");
    } else {
      path = this.free(folder ?? this.newFolder(from), "Untitled", ".md");
    }
    checkPath(path);
    const f = { text: "", mtimeMs: this.tick() };
    this.files.set(path, f);
    return this.note(path, f);
  }

  /** B-222: a note a character may link to (in the open vault, a Markdown note). */
  checkNote(s: AppSettings, path: string): void {
    this.ready(s);
    this.file(path);
    if (kindOf(path) !== "note") throw apiError("invalid_argument", "Only a Markdown note can be linked to a character.");
  }

  /** B-222: `Characters/<name>.md`, numbered from " 2" when taken (never replaced); returns its path. */
  createCharacterNote(s: AppSettings, name: string, text: string): string {
    this.ready(s);
    const stem = name.replace(/[/\\:*?"<>|#^[\]]/g, "-").trim().replace(/^\.+|\.+$/g, "") || "Character";
    for (let n = 0; ; n += 1) {
      const path = `Characters/${n === 0 ? stem : `${stem} ${n + 1}`}.md`;
      if (!this.files.has(path)) { this.files.set(path, { text, mtimeMs: this.tick() }); return path; }
    }
  }

  delete(s: AppSettings, path: string, permanent: boolean): VaultDeleteStatus {
    this.ready(s);
    this.file(path);
    if (!permanent && !this.trashAvailable) return "trash-unavailable";
    this.files.delete(path);
    if (!permanent) this.trashed.push(path);
    return permanent ? "deleted" : "trashed";
  }

  /** Obsidian's resolution, simplified: path from the vault folder, else a unique or nearest file name (case-insensitive). */
  resolveOne(from: string, target: string): string | null {
    const t = target.trim().replace(/^\//, "");
    if (!t) return this.files.has(from) ? from : null;
    const want = [`${t}.md`, t].map((c) => c.toLowerCase());
    const paths = [...this.files.keys()];
    for (const c of want) {
      const exact = paths.find((p) => p.toLowerCase() === c && c.includes("/"));
      if (exact) return exact;
      const matches = paths.filter((p) => p.toLowerCase() === c || p.toLowerCase().endsWith(`/${c}`));
      if (matches.length) {
        const dir = parentOf(from).toLowerCase();
        return [...matches].sort((a, b) => Number(parentOf(a).toLowerCase() !== dir) - Number(parentOf(b).toLowerCase() !== dir) || a.split("/").length - b.split("/").length || a.localeCompare(b))[0]!;
      }
    }
    return null;
  }

  resolve(s: AppSettings, from: string, targets: string[]): (string | null)[] {
    this.ready(s);
    return targets.map((t) => this.resolveOne(from, t));
  }

  private links(text: string): { target: string; index: number; length: number; targetStart: number; targetLength: number }[] {
    const body = text.replace(/```[\s\S]*?```/g, (m) => " ".repeat(m.length)).replace(/`[^`\n]*`/g, (m) => " ".repeat(m.length));
    return [...body.matchAll(WIKILINK)].map((m) => {
      const inner = m[2]!;
      const cut = Math.min(...["|", "#"].map((c) => (inner.includes(c) ? inner.indexOf(c) : inner.length)));
      const raw = inner.slice(0, cut).replace(/\\$/, "");
      return { target: raw.trim(), index: m.index!, length: m[0].length, targetStart: m.index! + m[1]!.length + 2, targetLength: raw.length };
    });
  }

  backlinks(s: AppSettings, path: string): VaultBacklink[] {
    this.ready(s);
    const out: VaultBacklink[] = [];
    for (const [from, f] of [...this.files.entries()].sort(([a], [b]) => a.toLowerCase().localeCompare(b.toLowerCase()))) {
      if (from === path || kindOf(from) !== "note") continue;
      const hits = this.links(f.text).filter((l) => l.target && this.resolveOne(from, l.target) === path);
      if (!hits.length) continue;
      const lines = f.text.split(/\r?\n/);
      const nums = [...new Set(hits.map((h) => f.text.slice(0, h.index).split("\n").length))].slice(0, 3);
      out.push({ path: from, count: hits.length, lines: nums.map((n) => ({ line: n, text: lines[n - 1]!.trim() })) });
    }
    return out;
  }

  private tagsOf(text: string): string[] {
    const fm = /^---\r?\n([\s\S]*?)\r?\n---/.exec(text);
    const fmTags = fm ? [...(/tags:\s*\[([^\]]*)\]/.exec(fm[1]!)?.[1]?.split(",") ?? [])].map((t) => t.trim()).filter(Boolean) : [];
    const body = fm ? text.slice(fm[0].length) : text;
    const code = body.replace(/```[\s\S]*?```/g, "").replace(/`[^`\n]*`/g, "");
    return [...[...code.matchAll(TAG)].map((m) => m[1]!), ...fmTags];
  }

  tags(s: AppSettings): VaultTag[] {
    this.ready(s);
    const counts = new Map<string, VaultTag>();
    for (const [p, f] of this.files) {
      if (kindOf(p) !== "note") continue;
      for (const tag of new Set(this.tagsOf(f.text).map((t) => t.toLowerCase()))) {
        const cur = counts.get(tag) ?? { tag, count: 0 };
        counts.set(tag, { ...cur, count: cur.count + 1 });
      }
    }
    return [...counts.values()].sort((a, b) => a.tag.localeCompare(b.tag));
  }

  search(s: AppSettings, query: string, tag?: string): VaultSearchHit[] {
    this.ready(s);
    const notes = [...this.files.entries()].filter(([p]) => kindOf(p) === "note").sort(([a], [b]) => a.toLowerCase().localeCompare(b.toLowerCase()));
    if (tag?.trim()) {
      const key = tag.replace(/^#/, "").toLowerCase();
      return notes.filter(([, f]) => this.tagsOf(f.text).some((t) => t.toLowerCase() === key || t.toLowerCase().startsWith(`${key}/`))).map(([path]) => ({ path, nameMatch: false, lines: [] }));
    }
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const hits = notes.map(([path, f]) => ({
      path, nameMatch: stripMd(fileName(path)).toLowerCase().includes(q),
      lines: f.text.split(/\r?\n/).map((text, i) => ({ line: i + 1, text: text.trim() })).filter((l) => l.text.toLowerCase().includes(q)).slice(0, 3),
    })).filter((h) => h.nameMatch || h.lines.length);
    return [...hits.filter((h) => h.nameMatch), ...hits.filter((h) => !h.nameMatch)];
  }

  private renamedPath(path: string, newName: string): string {
    const name = fileName(path);
    const ext = name.includes(".") ? name.slice(name.lastIndexOf(".")) : "";
    const stem = newName.toLowerCase().endsWith(ext.toLowerCase()) && ext ? newName.slice(0, -ext.length) : newName;
    checkName(stem);
    const to = join(parentOf(path), `${stem}${ext}`);
    if (to === path) throw apiError("invalid_argument", "That is already its name.");
    if (this.files.has(to)) throw apiError("invalid_argument", "A file with that name is already there.");
    return to;
  }

  private renameEdits(from: string, to: string): { path: string; text: string; next: string }[] {
    const out: { path: string; text: string; next: string }[] = [];
    for (const [p, f] of this.files) {
      if (kindOf(p) !== "note") continue;
      let next = "";
      let at = 0;
      for (const l of this.links(f.text)) {
        if (!l.target || this.resolveOne(p, l.target) !== from) continue;
        const keepExt = /\.\w+$/.test(l.target);
        const base = keepExt ? fileName(to) : stripMd(fileName(to));
        const replacement = l.target.includes("/") ? `${l.target.slice(0, l.target.lastIndexOf("/") + 1)}${base}` : base;
        next += f.text.slice(at, l.targetStart) + replacement;
        at = l.targetStart + l.targetLength;
      }
      if (at > 0) out.push({ path: p, text: f.text, next: next + f.text.slice(at) });
    }
    return out.sort((a, b) => a.path.toLowerCase().localeCompare(b.path.toLowerCase()));
  }

  renamePreview(s: AppSettings, path: string, newName: string): VaultRenamePlan {
    this.ready(s);
    this.file(path);
    const to = this.renamedPath(path, newName);
    const files = this.renameEdits(path, to).map(({ path: p, text, next }) => {
      const before = text.split("\n");
      const after = next.split("\n");
      const changes = before.map((b, i) => ({ line: i + 1, before: b, after: after[i] ?? "" })).filter((c) => c.before !== c.after);
      return { path: p, changes };
    });
    return { from: path, to, files };
  }

  rename(s: AppSettings, path: string, newName: string, updateLinks: boolean): VaultRenameDone {
    this.ready(s);
    const f = this.file(path);
    const to = this.renamedPath(path, newName);
    const edits = updateLinks ? this.renameEdits(path, to) : [];
    this.files.delete(path);
    this.files.set(to, f);
    for (const e of edits) {
      const target = e.path === path ? to : e.path;
      this.files.set(target, { text: e.next, mtimeMs: this.tick() });
    }
    return { to, updated: edits.map((e) => (e.path === path ? to : e.path)), skipped: [] };
  }

  image(s: AppSettings, path: string): string {
    this.ready(s);
    if (kindOf(path) !== "image") throw apiError("invalid_argument", "That file is not an image.");
    return this.file(path).text;
  }

  linkText(_from: string, to: string, embed: boolean): string {
    if (!this.files.has(to)) throw apiError("invalid_argument", "That file is not in your vault.");
    const name = fileName(to);
    const unique = [...this.files.keys()].filter((p) => fileName(p).toLowerCase() === name.toLowerCase()).length <= 1;
    const target = this.config.newLinkFormat === "absolute" || !unique ? to : name;
    if (this.config.useMarkdownLinks) return `${embed ? "!" : ""}[${embed ? "" : stripMd(name)}](${target.replace(/ /g, "%20")})`;
    return `${embed ? "!" : ""}[[${stripMd(target)}]]`;
  }

  attach(s: AppSettings, token: string, from: string): VaultAttachment {
    this.ready(s);
    const picked = this.picks.take(token, "vault-attachment");
    const name = fileName(picked);
    const dot = name.lastIndexOf(".");
    const p = this.config.attachmentFolderPath;
    const folder = p === "/" || !p ? "" : p === "./" ? parentOf(from) : p.startsWith("./") ? join(parentOf(from), p.slice(2)) : p;
    const path = this.free(folder, dot > 0 ? name.slice(0, dot) : name, dot > 0 ? name.slice(dot) : "");
    this.files.set(path, { text: IMAGE.test(path) ? PNG_URL : "", mtimeMs: this.tick() });
    return { path, link: this.linkText(from, path, IMAGE.test(path)) };
  }
}
