import { DOMParser as PMDOMParser, Fragment, Slice, type Node as PMNode } from "prosemirror-model";
import { normalizeDocJson } from "./doc";
import { richSchema } from "./schema";
import { fixText, normalizeRichText } from "./text";
import type { BlockJson, DocJson } from "./types";

// Paste rules (homebrew creator spec 7.1): pasted HTML is reduced to the schema (styles, fonts, links and images
// dropped; Word and Google Docs lists kept as lists); pasted plain text becomes paragraphs split on blank lines. Pasted
// text goes through the escape fix (a literal `\n` becomes a real line break) and the editor's normalization (NFC,
// no-break spaces, white space runs). Nothing pasted ever becomes a kept block.
// For the editor view (WB-10): `transformPastedHTML: cleanPastedHtml`, `clipboardTextParser: parsePastedText`,
// `transformPasted: normalizePastedSlice`.

/** Elements removed with their content. */
const DROP = "style, script, meta, link, title, img, svg, picture, video, audio, iframe, object, embed, colgroup, col";
/** Marker of a list paragraph in Word's clipboard HTML. */
const WORD_LIST = /mso-list\s*:/i;
/** Word's list label span (bullet or number). */
const WORD_LIST_LABEL = /mso-list\s*:\s*ignore/i;
/** A numbered label: "1.", "a)", "iv.". */
const ORDERED_LABEL = /^\s*(\d+|[a-z]|[ivxlcdm]+)[.)]/i;
const SPACE_RUNS = / {2,}/g;

function parseHtml(html: string): Document {
  return new DOMParser().parseFromString(`<!doctype html><html><body>${html}</body></html>`, "text/html");
}

function removeComments(root: Node): void {
  const doc = root.ownerDocument ?? (root as Document);
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_COMMENT);
  const comments: Node[] = [];
  while (walker.nextNode()) comments.push(walker.currentNode);
  for (const c of comments) c.parentNode?.removeChild(c);
}

/** Word list paragraphs (`style="mso-list:..."`) become `<ul>`/`<ol>` items; nesting levels are flattened. */
function convertWordLists(body: HTMLElement): void {
  const doc = body.ownerDocument;
  for (const p of Array.from(body.querySelectorAll("p"))) {
    if (!WORD_LIST.test(p.getAttribute("style") ?? "") || !p.isConnected) continue;
    const label = Array.from(p.querySelectorAll("span")).find((s) => WORD_LIST_LABEL.test(s.getAttribute("style") ?? ""));
    const ordered = label ? ORDERED_LABEL.test(label.textContent ?? "") : false;
    label?.remove();
    const prev = p.previousElementSibling;
    const tag = ordered ? "OL" : "UL";
    const list = prev?.tagName === tag && prev.hasAttribute("data-word-list") ? prev : doc.createElement(tag);
    if (list !== prev) {
      list.setAttribute("data-word-list", "");
      p.before(list);
    }
    const li = doc.createElement("li");
    li.append(...Array.from(p.childNodes));
    list.append(li);
    p.remove();
  }
}

/** Paragraphs and divs inside list items and cells (Docs, Word) are unwrapped: those hold one line of text. */
function unwrapInnerBlocks(body: HTMLElement): void {
  const doc = body.ownerDocument;
  for (const holder of Array.from(body.querySelectorAll("li, td, th"))) {
    const blocks = Array.from(holder.querySelectorAll(":scope > p, :scope > div"));
    blocks.forEach((block, i) => {
      if (i > 0) block.before(doc.createElement("br"));
      block.replaceWith(...Array.from(block.childNodes));
    });
  }
}

/** Reduce clipboard HTML to what the schema can hold (the editor's `transformPastedHTML`). */
export function cleanPastedHtml(html: string): string {
  const doc = parseHtml(html);
  const body = doc.body;
  for (const el of Array.from(body.querySelectorAll(DROP))) el.remove();
  removeComments(body);
  convertWordLists(body);
  unwrapInnerBlocks(body);
  for (const el of Array.from(body.querySelectorAll("[data-word-list]"))) el.removeAttribute("data-word-list");
  return body.innerHTML;
}

function isCell(node: PMNode): boolean {
  return node.type.name === "table_cell" || node.type.name === "table_header";
}

/** Pasted text made canonical: escape fix (a fixed line break becomes a hard break), the editor's normalization,
 * single spaces; kept blocks dropped. Adjacent text with the same marks is joined by `Fragment.fromArray`. */
function cleanFragment(fragment: Fragment, inCell: boolean): Fragment {
  const nodes: PMNode[] = [];
  fragment.forEach((node) => {
    if (node.type.name === "kept") return;
    if (!node.isText) {
      nodes.push(node.copy(cleanFragment(node.content, inCell || isCell(node))));
      return;
    }
    const lines = normalizeRichText(fixText(node.text ?? ""), inCell).replace(SPACE_RUNS, " ").split("\n");
    lines.forEach((line, i) => {
      if (i > 0) nodes.push(richSchema.nodes.hard_break.create(null, null, node.marks));
      if (line !== "") nodes.push(richSchema.text(line, node.marks));
    });
  });
  return Fragment.fromArray(nodes);
}

/** Pasted content made canonical (the editor's `transformPasted`). */
export function normalizePastedSlice(slice: Slice): Slice {
  return new Slice(cleanFragment(slice.content, false), slice.openStart, slice.openEnd);
}

function pastedDoc(doc: PMNode): PMNode {
  const cleaned = doc.copy(cleanFragment(doc.content, false));
  return richSchema.nodeFromJSON(normalizeDocJson(cleaned.toJSON() as DocJson));
}

/** Clipboard HTML as a whole document (tests and "paste into an empty description"). */
export function pastedHtmlToDoc(html: string): PMNode {
  const dom = parseHtml(cleanPastedHtml(html)).body;
  return pastedDoc(PMDOMParser.fromSchema(richSchema).parse(dom));
}

/** Clipboard HTML as a slice to insert at the cursor. */
export function parsePastedHtml(html: string): Slice {
  const dom = parseHtml(cleanPastedHtml(html)).body;
  return normalizePastedSlice(PMDOMParser.fromSchema(richSchema).parseSlice(dom));
}

/** Plain text as paragraph JSON: escape fix first (literal `\n` is a line break), then paragraphs on blank lines;
 * single line breaks and tabs inside a paragraph become spaces (text copied from a PDF is hard-wrapped). */
function textParagraphs(text: string): BlockJson[] {
  const fixed = fixText(text.replace(/\r\n?/g, "\n"));
  return fixed
    .split(/\n[ \t\u{a0}]*\n/u)
    .map((part) => part.replace(/\s*\n\s*/g, " ").replace(/\t/g, " ").trim())
    .filter((part) => part !== "")
    .map((part) => ({ type: "paragraph", attrs: { joined: false, bare: false }, content: [{ type: "text", text: part }] }));
}

/** Plain clipboard text as a whole document. */
export function pastedTextToDoc(text: string): PMNode {
  return pastedDoc(richSchema.nodeFromJSON({ type: "doc", content: textParagraphs(text) }));
}

/** Plain clipboard text as a slice to insert at the cursor (the editor's `clipboardTextParser`). */
export function parsePastedText(text: string): Slice {
  const doc = pastedTextToDoc(text);
  return doc.childCount === 0 ? Slice.empty : new Slice(doc.content, 1, 1);
}
