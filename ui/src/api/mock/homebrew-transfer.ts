import type { ContentPack, HomebrewElementSummary, HomebrewForm, ImportFileInput } from "../types";
import { apiError } from "./errors";
import { fromBase64, unzipFiles, zipFiles } from "./pack-file";

/** A homebrew pack of the web mock as it travels in a `.borealis-pack.zip` (spec 14.2). */
export interface MockPackData {
  pack: ContentPack;
  code: string;
  elements: [string, { summary: HomebrewElementSummary; form: HomebrewForm }][];
  appends: [string, string[]][];
}

const CREATOR = "creator.json";
const CODE_MAX = 8;

/** `<Pack name>.borealis-pack.zip`, as `crate::ids::file_stem` names it. */
export const packFileName = (name: string): string => `${name.replace(/[^\p{L}\p{N}-]/gu, "_").slice(0, 60) || "pack"}.borealis-pack.zip`;

/** The pack file: `pack.json`, `content.json` (the mock's elements stand in for the content file) and `creator.json`, in the
 * pack's folder. Elements marked `copiedText` lose their description with `leaveOut` (HC8). */
export function packFile(data: MockPackData, leaveOut: boolean): { bytes: Uint8Array; leftOut: number } {
  let leftOut = 0;
  const elements = data.elements.map(([id, e]): MockPackData["elements"][number] => {
    if (!leaveOut || !e.summary.copiedText) return [id, e];
    leftOut += 1;
    return [id, { summary: { ...e.summary, copiedText: false }, form: { ...e.form, fields: { ...e.form.fields, description: "" } } }];
  });
  const creator = { format: "borealis-creator/1", code: data.code, createdWith: "mock", retiredIds: [],
    elements: Object.fromEntries(data.elements.map(([id, e]) => [id, { kind: e.summary.kind, createdAt: "", nonSrdText: e.summary.copiedText }])) };
  const dir = data.pack.id;
  const bytes = zipFiles({
    [`${dir}/pack.json`]: JSON.stringify({ format: "borealis-pack/1", id: data.pack.id, name: data.pack.name, version: data.pack.version, origin: "custom",
      rulesets: data.pack.rulesets, files: [{ path: "content.json", kind: "content" }] }, null, 2),
    [`${dir}/content.json`]: JSON.stringify({ format: "borealis-content/1", mock: { ...data, elements } }, null, 2),
    [`${dir}/${CREATOR}`]: JSON.stringify(creator, null, 2),
  });
  return { bytes, leftOut };
}

/** The pack in an imported file, when it is one `.zip` with a `creator.json` (the creator marker); null otherwise. */
export function readPackFile(files: readonly ImportFileInput[]): MockPackData | null {
  const [file, ...more] = files;
  if (!file || more.length || !/\.zip$/i.test(file.fileName)) return null;
  let entries: Record<string, string> | null;
  try { entries = unzipFiles(fromBase64(file.contentBase64)); } catch { entries = null; }
  const marker = Object.keys(entries ?? {}).find((n) => n.split("/").pop() === CREATOR);
  if (!entries || !marker) return null;
  const dir = marker.slice(0, -CREATOR.length);
  try {
    const content = JSON.parse(entries[`${dir}content.json`] ?? "") as { mock?: MockPackData };
    if (!content.mock) throw new Error("no content");
    return content.mock;
  } catch {
    throw apiError("invalid_argument", "This pack file is damaged: its content.json can't be read. Nothing was imported.");
  }
}

/** The next free code: the code's letters and a number (`EI` -> `EI2`), at most 8 long. */
export function copyCode(code: string, taken: ReadonlySet<string>): string {
  const stem = code.replace(/\d+$/, "") || code;
  for (let n = 2; ; n++) {
    const tail = String(n);
    const next = `${stem.slice(0, CODE_MAX - tail.length)}${tail}`;
    if (!taken.has(next)) return next;
  }
}

/** The pack as a copy with `code` and `name`: every `ID_<OLD>_` id given again, references following (spec 14.2). */
export function copyOf(data: MockPackData, code: string, name: string): MockPackData {
  const text = JSON.stringify({ elements: data.elements, appends: data.appends }).replace(new RegExp(`\\bID_${data.code}_`, "g"), `ID_${code}_`);
  const moved = JSON.parse(text) as Pick<MockPackData, "elements" | "appends">;
  const id = `homebrew-${code.toLowerCase()}`;
  // the book is renamed, and every element that names it follows
  const elements = moved.elements.map(([eid, e]): MockPackData["elements"][number] =>
    [eid, e.form.fields.source === data.pack.name ? { ...e, form: { ...e.form, fields: { ...e.form.fields, source: name } } } : e]);
  return { appends: moved.appends, elements, code, pack: { ...data.pack, id, name, sourceIds: [`source-${id}`] } };
}
