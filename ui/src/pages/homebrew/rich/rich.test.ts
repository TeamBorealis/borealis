import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { DOMSerializer } from "prosemirror-model";
import { fixDocJson, lintDocJson, normalizeDocJson } from "./doc";
import { parsePastedHtml, parsePastedText, pastedHtmlToDoc, pastedTextToDoc } from "./paste";
import { richSchema } from "./schema";
import { decodeEntityAt, fixText, lintText, normalizePlainText } from "./text";
import type { DocJson, EscapeFinding } from "./types";

// The shared fixtures: the Rust tree (`crates/borealis-content/tests/rich_text_fixtures.rs`) passes the same file.
interface Fixtures {
  roundtrip: { name: string; html: string; doc: DocJson }[];
  normalize: { name: string; doc: DocJson; normalized: DocJson }[];
  lint: { text: string; findings: Omit<EscapeFinding, "path">[]; fixed: string }[];
  plain: { text: string; normalized: string }[];
  doc_lint: { name: string; doc: DocJson; findings: EscapeFinding[]; fixed: DocJson; fixed_html: string }[];
  paste: { name: string; html?: string; text?: string; doc: DocJson; rendered: string }[];
}

const FIXTURES = JSON.parse(
  readFileSync(resolve(__dirname, "../../../../../crates/borealis-content/tests/fixtures/rich_text.json"), "utf8"),
) as Fixtures;

describe("shared fixtures (Rust and UI agree)", () => {
  it.each(FIXTURES.roundtrip.map((c) => [c.name, c] as const))("the schema holds the Rust tree: %s", (_, c) => {
    const node = richSchema.nodeFromJSON(c.doc);
    node.check();
    expect(node.toJSON()).toEqual(c.doc);
  });

  it.each(FIXTURES.normalize.map((c) => [c.name, c] as const))("normalizes like Rust: %s", (_, c) => {
    const normalized = normalizeDocJson(c.doc);
    expect(normalized).toEqual(c.normalized);
    expect(richSchema.nodeFromJSON(normalized).toJSON()).toEqual(c.normalized);
  });

  it("lints and fixes text like Rust", () => {
    expect(FIXTURES.lint.length).toBeGreaterThan(10);
    for (const c of FIXTURES.lint) {
      const findings = lintText(c.text).map(({ kind, found, replacement }) => ({ kind, found, replacement }));
      expect(findings, JSON.stringify(c.text)).toEqual(c.findings);
      expect(fixText(c.text), JSON.stringify(c.text)).toBe(c.fixed);
    }
    for (const c of FIXTURES.plain) expect(normalizePlainText(c.text)).toBe(c.normalized);
  });

  it.each(FIXTURES.doc_lint.map((c) => [c.name, c] as const))("lints and fixes documents like Rust: %s", (_, c) => {
    expect(lintDocJson(c.doc)).toEqual(c.findings);
    expect(fixDocJson(c.doc)).toEqual(c.fixed);
  });

  it.each(FIXTURES.paste.map((c) => [c.name, c] as const))("paste gives the tree Rust writes: %s", (_, c) => {
    const node = c.html !== undefined ? pastedHtmlToDoc(c.html) : pastedTextToDoc(c.text ?? "");
    expect(node.toJSON()).toEqual(c.doc);
  });
});

describe("schema", () => {
  it("writes bold outside italic with the recorded tag spelling", () => {
    const doc = richSchema.nodeFromJSON({
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [
            { type: "text", text: "x", marks: [{ type: "italic", attrs: { tag: "em" } }, { type: "bold", attrs: { tag: "strong" } }] },
          ],
        },
      ],
    });
    const dom = DOMSerializer.fromSchema(richSchema).serializeFragment(doc.content);
    const div = document.createElement("div");
    div.append(dom);
    expect(div.innerHTML).toBe("<p><strong><em>x</em></strong></p>");
  });

  it("shows a kept block rendered and sanitized, and never parses one from HTML", () => {
    const doc = richSchema.nodeFromJSON({
      type: "doc",
      content: [{ type: "kept", attrs: { joined: false, html: "<h3 onclick=\"x()\">T</h3><script>bad()</script>", reason: "outside_palette" } }],
    });
    const div = document.createElement("div");
    div.append(DOMSerializer.fromSchema(richSchema).serializeFragment(doc.content));
    expect(div.innerHTML).toContain("<h3>T</h3>");
    expect(div.innerHTML).not.toContain("onclick");
    expect(div.innerHTML).not.toContain("<script");
    const pasted = pastedHtmlToDoc(div.innerHTML);
    pasted.descendants((n) => {
      expect(n.type.name).not.toBe("kept");
    });
  });

  it("refuses content the Rust tree cannot hold", () => {
    expect(() => richSchema.nodeFromJSON({ type: "doc", content: [{ type: "bullet_list" }] }).check()).toThrow();
    expect(() => richSchema.nodeFromJSON({ type: "doc", content: [{ type: "image" }] })).toThrow();
  });
});

describe("paste rules", () => {
  it("plain text splits paragraphs on blank lines, and a literal \\n counts as one", () => {
    const slice = parsePastedText("One\\n\\nTwo\nstill two");
    expect(slice.content.childCount).toBe(2);
    expect(slice.openStart).toBe(1);
    expect(slice.content.child(1).textContent).toBe("Two still two");
    expect(parsePastedText("  \n ").size).toBe(0);
  });

  it("smart quotes and other characters stay characters", () => {
    const text = "\u{201c}It\u{2019}s\u{201d} \u{2014} caf\u{e9} \u{5d0}";
    expect(pastedTextToDoc(text).textContent).toBe(text);
  });

  it("a pasted HTML slice is cleaned of escapes, styles and kept blocks", () => {
    const slice = parsePastedHtml("<span style=\"font-weight:700\">Bold</span> &amp;amp; it\\'s");
    const doc = richSchema.topNodeType.createAndFill(null, slice.content) ?? richSchema.node("paragraph", null, slice.content);
    expect(doc.textContent).toBe("Bold & it's");
  });

  it("decodes the same references as Rust", () => {
    expect(decodeEntityAt("&rsquo;x")).toEqual(["\u{2019}", 7]);
    expect(decodeEntityAt("&#x41;")).toEqual(["A", 6]);
    expect(decodeEntityAt("&#+65;")).toBeNull();
    expect(decodeEntityAt("&averyveryverylongname;")).toBeNull();
  });
});
