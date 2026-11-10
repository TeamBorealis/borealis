import { fixText, lintText, normalizeRichText } from "./text";
import type {
  BlockJson,
  CellJson,
  DocJson,
  EscapeFinding,
  InlineJson,
  ListItemJson,
  MarkJson,
  TableRowJson,
} from "./types";

// Document-level rules, the twin of `Doc::normalized`, `rich::lint` and `rich::fix` in Rust (same fixtures).

function markRank(mark: MarkJson): number {
  return mark.type === "bold" ? 0 : 1;
}

function normalizeMarks(marks: readonly MarkJson[] | undefined): MarkJson[] {
  const out: MarkJson[] = [];
  for (const mark of marks ?? []) {
    if (out.some((m) => m.type === mark.type)) continue;
    out.push(
      mark.type === "bold"
        ? { type: "bold", attrs: { tag: mark.attrs?.tag ?? "b" } }
        : { type: "italic", attrs: { tag: mark.attrs?.tag ?? "i" } },
    );
  }
  return out.sort((a, b) => markRank(a) - markRank(b));
}

function sameMarks(a: readonly MarkJson[], b: readonly MarkJson[]): boolean {
  return a.length === b.length && a.every((m, i) => m.type === b[i]?.type && m.attrs.tag === b[i]?.attrs.tag);
}

function withMarks<T extends object>(node: T, marks: MarkJson[]): T {
  return marks.length > 0 ? { ...node, marks } : node;
}

function withContent<T extends object, C>(node: T, content: C[]): T {
  return content.length > 0 ? { ...node, content } : node;
}

function normalizeInline(nodes: readonly InlineJson[] | undefined, inCell: boolean): InlineJson[] {
  const out: InlineJson[] = [];
  for (const node of nodes ?? []) {
    const marks = normalizeMarks(node.marks);
    if (node.type === "hard_break") {
      out.push(withMarks({ type: "hard_break" }, marks));
      continue;
    }
    const text = normalizeRichText(node.text, inCell);
    if (text === "") continue;
    const prev = out[out.length - 1];
    if (prev?.type === "text" && sameMarks(prev.marks ?? [], marks)) {
      out[out.length - 1] = { ...prev, text: prev.text + text };
      continue;
    }
    out.push(withMarks({ type: "text", text }, marks));
  }
  return out;
}

function isEmptyContainer(block: BlockJson): boolean {
  switch (block.type) {
    case "bullet_list":
    case "ordered_list":
    case "table":
      return (block.content ?? []).length === 0;
    case "blockquote":
      return (block.content ?? []).every(isEmptyContainer);
    default:
      return false;
  }
}

function setJoined(block: BlockJson, joined: boolean): BlockJson {
  return { ...block, attrs: { ...block.attrs, joined } } as BlockJson;
}

function firstNotJoined(blocks: BlockJson[]): BlockJson[] {
  return blocks.map((b, i) => (i === 0 ? setJoined(b, false) : b));
}

function normalizeItems(items: readonly ListItemJson[] | undefined): ListItemJson[] {
  return (items ?? []).map((item) => withContent({ type: "list_item" as const }, normalizeInline(item.content, false)));
}

function normalizeRows(rows: readonly TableRowJson[] | undefined, sections: boolean): TableRowJson[] {
  let inHead = sections;
  return (rows ?? []).map((row) => {
    inHead = inHead && Boolean(row.attrs?.head);
    const cells: CellJson[] = (row.content ?? []).map((cell) =>
      withContent({ type: cell.type }, normalizeInline(cell.content, true)),
    );
    return withContent({ type: "table_row" as const, attrs: { head: inHead } }, cells);
  });
}

function normalizeBlock(block: BlockJson, top: boolean): BlockJson {
  const joined = Boolean(block.attrs?.joined);
  switch (block.type) {
    case "paragraph":
      return withContent(
        { type: "paragraph", attrs: { joined, bare: Boolean(block.attrs?.bare) && top } },
        normalizeInline(block.content, false),
      );
    case "heading":
      return withContent(
        { type: "heading", attrs: { joined, level: (block.attrs?.level ?? 3) >= 4 ? 4 : 3 } },
        normalizeInline(block.content, false),
      );
    case "bullet_list":
    case "ordered_list":
      return withContent({ type: block.type, attrs: { joined } }, normalizeItems(block.content));
    case "table": {
      const sections = block.attrs?.sections ?? true;
      return withContent({ type: "table", attrs: { joined, sections } }, normalizeRows(block.content, sections));
    }
    case "blockquote": {
      const children = (block.content ?? []).filter((b) => !isEmptyContainer(b)).map((b) => normalizeBlock(b, false));
      return withContent({ type: "blockquote", attrs: { joined } }, firstNotJoined(children));
    }
    case "kept":
      return {
        type: "kept",
        attrs: { joined, html: block.attrs?.html ?? "", reason: block.attrs?.reason ?? "outside_palette" },
      };
  }
}

/** The canonical form of an editor document (`Doc::normalized` in Rust). */
export function normalizeDocJson(doc: DocJson): DocJson {
  let blocks = (doc.content ?? []).filter((b) => !isEmptyContainer(b)).map((b) => normalizeBlock(b, true));
  if (blocks.length > 1) {
    blocks = blocks.map((b) => (b.type === "paragraph" ? { ...b, attrs: { ...b.attrs, bare: false } } : b));
  }
  const only = blocks[0];
  if (blocks.length === 1 && only?.type === "paragraph" && (only.content ?? []).length === 0) blocks = [];
  return withContent({ type: "doc" as const }, firstNotJoined(blocks));
}

function lintInline(nodes: readonly InlineJson[] | undefined, path: number[], out: EscapeFinding[]): void {
  (nodes ?? []).forEach((node, i) => {
    if (node.type !== "text") return;
    for (const f of lintText(node.text)) {
      out.push({ path: [...path, i], kind: f.kind, found: f.found, replacement: f.replacement });
    }
  });
}

function lintBlock(block: BlockJson, path: number[], out: EscapeFinding[]): void {
  switch (block.type) {
    case "paragraph":
    case "heading":
      lintInline(block.content, path, out);
      return;
    case "bullet_list":
    case "ordered_list":
      (block.content ?? []).forEach((item, i) => lintInline(item.content, [...path, i], out));
      return;
    case "table":
      (block.content ?? []).forEach((row, r) =>
        (row.content ?? []).forEach((cell, c) => lintInline(cell.content, [...path, r, c], out)),
      );
      return;
    case "blockquote":
      (block.content ?? []).forEach((child, i) => lintBlock(child, [...path, i], out));
      return;
    case "kept":
      return;
  }
}

/** Escape artefacts in the editable text of `doc` (kept blocks are the original bytes and are not looked at). */
export function lintDocJson(doc: DocJson): EscapeFinding[] {
  const out: EscapeFinding[] = [];
  (doc.content ?? []).forEach((block, i) => lintBlock(block, [i], out));
  return out;
}

function fixInline(nodes: readonly InlineJson[] | undefined): InlineJson[] {
  return (nodes ?? []).flatMap((node): InlineJson[] => {
    if (node.type !== "text") return [node];
    return fixText(node.text)
      .split("\n")
      .flatMap((line, i): InlineJson[] => {
        const text: InlineJson = { ...node, text: line };
        return i === 0 ? [text] : [withMarks({ type: "hard_break" }, node.marks ?? []), text];
      });
  });
}

function fixBlock(block: BlockJson): BlockJson {
  switch (block.type) {
    case "paragraph":
    case "heading":
      return { ...block, content: fixInline(block.content) };
    case "bullet_list":
    case "ordered_list":
      return { ...block, content: (block.content ?? []).map((item) => ({ ...item, content: fixInline(item.content) })) };
    case "table":
      return {
        ...block,
        content: (block.content ?? []).map((row) => ({
          ...row,
          content: (row.content ?? []).map((cell) => ({ ...cell, content: fixInline(cell.content) })),
        })),
      };
    case "blockquote":
      return { ...block, content: (block.content ?? []).map(fixBlock) };
    case "kept":
      return block;
  }
}

/** `doc` with every artefact fixed (a fixed line break becomes a hard break), normalized (`rich::fix` in Rust). */
export function fixDocJson(doc: DocJson): DocJson {
  return normalizeDocJson({ type: "doc", content: (doc.content ?? []).map(fixBlock) });
}
