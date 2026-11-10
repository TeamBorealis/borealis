import { Schema, type DOMOutputSpec, type Node as PMNode } from "prosemirror-model";
import { sanitizeHtml } from "../../../components/SafeHtml";

// ProseMirror schema mirroring the Rust rich-text tree (homebrew creator spec 7.1-7.2). Node and mark names, attrs and
// content rules match `borealis_content::rich` one for one, so `doc.toJSON()` is what the backend deserializes and
// what it sends back. The palette is p, b/strong, i/em, br, ul/ol/li, h3/h4, table/thead/tbody/tr/th/td, blockquote.
// `kept` holds formatting outside the palette as its original HTML: shown rendered, read-only, removable; nothing
// pasted ever becomes a kept block (no parse rule).

const joined = { joined: { default: false } };

/** Block types a blockquote may hold. */
const QUOTE_CONTENT = "(paragraph | heading | bullet_list | ordered_list)+";

function headingLevel(tag: string): number {
  return tag === "h4" || tag === "h5" || tag === "h6" ? 4 : 3;
}

/** Bold from a pasted style value ("bold", "700"...). */
const BOLD_WEIGHT = /^(bold(er)?|[5-9]\d{2,})$/;

export const richSchema = new Schema({
  nodes: {
    doc: { content: "block*" },
    paragraph: {
      group: "block",
      content: "inline*",
      attrs: { ...joined, bare: { default: false } },
      parseDOM: [{ tag: "p" }],
      toDOM: (): DOMOutputSpec => ["p", 0],
    },
    heading: {
      group: "block",
      content: "inline*",
      defining: true,
      attrs: { ...joined, level: { default: 3 } },
      parseDOM: ["h1", "h2", "h3", "h4", "h5", "h6"].map((tag) => ({ tag, attrs: { level: headingLevel(tag) } })),
      toDOM: (node): DOMOutputSpec => [node.attrs.level === 4 ? "h4" : "h3", 0],
    },
    bullet_list: {
      group: "block",
      content: "list_item+",
      attrs: joined,
      parseDOM: [{ tag: "ul" }],
      toDOM: (): DOMOutputSpec => ["ul", 0],
    },
    ordered_list: {
      group: "block",
      content: "list_item+",
      attrs: joined,
      parseDOM: [{ tag: "ol" }],
      toDOM: (): DOMOutputSpec => ["ol", 0],
    },
    list_item: {
      content: "inline*",
      defining: true,
      parseDOM: [{ tag: "li" }],
      toDOM: (): DOMOutputSpec => ["li", 0],
    },
    table: {
      group: "block",
      content: "table_row+",
      isolating: true,
      attrs: { ...joined, sections: { default: true } },
      parseDOM: [{ tag: "table" }],
      toDOM: (): DOMOutputSpec => ["table", ["tbody", 0]],
    },
    table_row: {
      content: "(table_cell | table_header)*",
      attrs: { head: { default: false } },
      parseDOM: [{ tag: "tr", getAttrs: (dom: HTMLElement) => ({ head: dom.parentElement?.tagName === "THEAD" }) }],
      toDOM: (): DOMOutputSpec => ["tr", 0],
    },
    table_cell: {
      content: "inline*",
      isolating: true,
      parseDOM: [{ tag: "td" }],
      toDOM: (): DOMOutputSpec => ["td", 0],
    },
    table_header: {
      content: "inline*",
      isolating: true,
      parseDOM: [{ tag: "th" }],
      toDOM: (): DOMOutputSpec => ["th", 0],
    },
    blockquote: {
      group: "block",
      content: QUOTE_CONTENT,
      attrs: joined,
      parseDOM: [{ tag: "blockquote" }],
      toDOM: (): DOMOutputSpec => ["blockquote", 0],
    },
    kept: {
      group: "block",
      atom: true,
      selectable: true,
      attrs: { ...joined, html: { default: "" }, reason: { default: "outside_palette" } },
      toDOM: (node: PMNode) => {
        const dom = document.createElement("div");
        dom.className = "rich rich-kept";
        dom.innerHTML = sanitizeHtml(String(node.attrs.html));
        return dom;
      },
    },
    text: { group: "inline" },
    hard_break: {
      group: "inline",
      inline: true,
      selectable: false,
      parseDOM: [{ tag: "br" }],
      toDOM: (): DOMOutputSpec => ["br"],
    },
  },
  // Order is the nesting rank: bold is written outside italic, as the Rust writer does.
  marks: {
    bold: {
      attrs: { tag: { default: "b" } },
      parseDOM: [
        { tag: "b", getAttrs: (dom: HTMLElement) => dom.style.fontWeight !== "normal" && null },
        { tag: "strong" },
        { style: "font-weight", getAttrs: (value: string) => BOLD_WEIGHT.test(value) && null },
      ],
      toDOM: (mark): DOMOutputSpec => [String(mark.attrs.tag), 0],
    },
    italic: {
      attrs: { tag: { default: "i" } },
      parseDOM: [{ tag: "i" }, { tag: "em" }, { style: "font-style=italic" }],
      toDOM: (mark): DOMOutputSpec => [String(mark.attrs.tag), 0],
    },
  },
});
