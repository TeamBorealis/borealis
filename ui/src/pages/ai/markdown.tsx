import type { ReactNode } from "react";

/**
 * Minimal, safe markdown for model output. Everything is built as React elements (text nodes), never
 * as HTML, so a `<script>` or `<img onerror>` payload is shown as literal text. Supported: headings,
 * paragraphs, bullet/numbered lists, fenced code, pipe tables, **bold**, *italic*, `code`. `[label](url)` keeps only
 * its label: model output never produces a real hyperlink; citations come from `[[ID]]` markers via
 * `renderText`. Unclosed markers (a half-streamed `**`) format to the end of the line instead of
 * showing the raw marker.
 */
export type RenderText = (text: string, key: string) => ReactNode;

type Block =
  | { kind: "heading"; text: string }
  | { kind: "para"; text: string }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "code"; text: string }
  | { kind: "table"; head: string[]; rows: string[][] };

const HEADING = /^\s{0,3}#{1,6}(?:\s+(.*))?$/;
const BULLET = /^\s{0,3}[-*+]\s+(.*)$/;
const NUMBERED = /^\s{0,3}\d{1,3}[.)]\s+(.*)$/;
const FENCE = /^\s{0,3}```/;
const TABLE_ROW = /^\s{0,3}\|.*\|\s*$/;
const TABLE_RULE = /^\s{0,3}\|(?:\s*:?-{3,}:?\s*\|)+\s*$/;

/** `| a | b |` -> ["a", "b"] (an escaped `\|` stays in its cell). */
function tableCells(line: string): string[] {
  return line.trim().slice(1, -1).split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, "|"));
}

const LINK = /\[([^\]\n]+)\]\([^)\n]*\)/g;

/** Drop a trailing half-typed `[[ID` marker so it never flashes while streaming. */
function trimPartialMarker(text: string): string {
  return text.replace(/\[\[[^\]\n]*\]?$/, "");
}

export function parseBlocks(source: string): Block[] {
  const blocks: Block[] = [];
  const lines = trimPartialMarker(source).replace(/\r\n?/g, "\n").split("\n");
  let para: string[] = [];
  const flush = () => { if (para.length) blocks.push({ kind: "para", text: para.join(" ") }); para = []; };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (FENCE.test(line)) {
      flush();
      const code: string[] = [];
      for (i++; i < lines.length && !FENCE.test(lines[i]!); i++) code.push(lines[i]!);
      blocks.push({ kind: "code", text: code.join("\n") });
      continue;
    }
    if (TABLE_ROW.test(line) && TABLE_RULE.test(lines[i + 1] ?? "")) {
      flush();
      const head = tableCells(line);
      const rows: string[][] = [];
      for (i += 2; i < lines.length && TABLE_ROW.test(lines[i]!); i++) rows.push(tableCells(lines[i]!));
      i--;
      blocks.push({ kind: "table", head, rows });
      continue;
    }
    const h = HEADING.exec(line);
    const b = BULLET.exec(line);
    const n = NUMBERED.exec(line);
    if (h) { flush(); if (h[1]?.trim()) blocks.push({ kind: "heading", text: h[1].trim() }); }
    else if (b || n) {
      flush();
      const ordered = !!n;
      const text = (b ?? n)![1]!;
      const last = blocks[blocks.length - 1];
      if (last?.kind === "list" && last.ordered === ordered) last.items.push(text);
      else blocks.push({ kind: "list", ordered, items: [text] });
    } else if (!line.trim()) flush();
    else para.push(line.trim());
  }
  flush();
  return blocks;
}

/** Find the next closing `marker` that follows a non-space char; -1 when absent. */
function closeIndex(text: string, marker: string, from: number): number {
  for (let i = text.indexOf(marker, from); i !== -1; i = text.indexOf(marker, i + 1)) {
    if (i > from && !/\s/.test(text[i - 1]!)) return i;
  }
  return -1;
}

export function renderInline(raw: string, renderText: RenderText, prefix = "i"): ReactNode[] {
  const text = raw.replace(LINK, "$1");
  const out: ReactNode[] = [];
  let buf = "";
  let n = 0;
  const key = () => `${prefix}-${n++}`;
  const flushText = () => { if (buf) { const k = key(); out.push(renderText(buf, k)); buf = ""; } };
  const wrap = (marker: string, at: number, tag: "strong" | "em" | "code") => {
    const from = at + marker.length;
    const close = closeIndex(text, marker, from);
    const end = close === -1 ? text.length : close;
    const inner = text.slice(from, end);
    flushText();
    const k = key();
    if (tag === "code") out.push(<code key={k}>{inner}</code>);
    else {
      const Tag = tag;
      out.push(<Tag key={k}>{renderInline(inner, renderText, k)}</Tag>);
    }
    return close === -1 ? text.length : close + marker.length;
  };
  for (let i = 0; i < text.length;) {
    const rest = text.slice(i);
    if (rest.startsWith("**") && !/^\*\*\s/.test(rest)) i = wrap("**", i, "strong");
    else if (rest.startsWith("`")) i = wrap("`", i, "code");
    else if (rest.startsWith("*") && !/^\*\s/.test(rest) && rest.length > 1) i = wrap("*", i, "em");
    else { buf += text[i]; i++; }
  }
  flushText();
  return out;
}

export function Markdown({ text, renderText }: { text: string; renderText: RenderText }) {
  return (
    <div className="v5-md">
      {parseBlocks(text).map((b, i) => {
        const k = `b${i}`;
        switch (b.kind) {
          case "heading": return <h3 key={k} className="v5-md-h">{renderInline(b.text, renderText, k)}</h3>;
          case "para": return <p key={k}>{renderInline(b.text, renderText, k)}</p>;
          case "code": return <pre key={k}>{b.text}</pre>;
          case "table": return (
            <table key={k} className="v5-md-table">
              <thead><tr>{b.head.map((c, j) => <th key={j}>{renderInline(c, renderText, `${k}-h${j}`)}</th>)}</tr></thead>
              <tbody>{b.rows.map((r, ri) => <tr key={ri}>{b.head.map((_, j) => <td key={j}>{renderInline(r[j] ?? "", renderText, `${k}-${ri}-${j}`)}</td>)}</tr>)}</tbody>
            </table>
          );
          case "list": {
            const items = b.items.map((it, j) => <li key={j}>{renderInline(it, renderText, `${k}-${j}`)}</li>);
            return b.ordered ? <ol key={k}>{items}</ol> : <ul key={k}>{items}</ul>;
          }
        }
      })}
    </div>
  );
}
