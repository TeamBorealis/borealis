import { splitFrontMatter } from "./frontMatter";

/** A task line: optional `>` quote markers, a list marker, then `[c]`. Group 1 runs up to the `[`, group 2 is the mark. */
const TASK_LINE = /^((?:[ \t]*>)*[ \t]*(?:[-*+]|\d+[.)])[ \t]+\[)(.)\]/;
const HEADING = /^(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/;

const normal = (s: string) => s.trim().toLowerCase().replace(/\s+/g, " ");
const escapeRegExp = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/**
 * Tick or untick the task on 0-based `line` of `text`: exactly one character changes (`[ ]` <-> `[x]`, any other mark
 * becomes `[ ]`). Null when that line is not a task.
 */
export function toggleTask(text: string, line: number): string | null {
  const lines = text.split("\n");
  const current = lines[line];
  const m = current === undefined ? null : TASK_LINE.exec(current);
  if (!current || !m) return null;
  const mark = m[2] === " " ? "x" : " ";
  const at = m[1]!.length;
  const edited = current.slice(0, at) + mark + current.slice(at + 1);
  return [...lines.slice(0, line), edited, ...lines.slice(line + 1)].join("\n");
}

/** The heading level of a line, or 0. */
function headingLevel(line: string): number {
  return HEADING.exec(line)?.[1]?.length ?? 0;
}

/**
 * The part of a note an embed shows: the whole note without its front matter, a heading's section (`Heading`, down to the
 * next heading of the same or a higher level), or a block (`^id`: the paragraph that ends with it). Best effort: an unknown
 * heading or block shows the whole note.
 */
export function extractSection(text: string, subpath: string | null): string {
  const fm = splitFrontMatter(text);
  const body = fm ? text.slice(fm.end) : text;
  if (!subpath) return body;
  const lines = body.split("\n");
  if (subpath.startsWith("^")) {
    const marker = new RegExp(`(^|\\s)\\^${escapeRegExp(subpath.slice(1))}\\s*$`);
    const at = lines.findIndex((l) => marker.test(l));
    if (at < 0) return body;
    let start = at;
    while (start > 0 && lines[start - 1]!.trim() !== "") start -= 1;
    return lines.slice(start, at + 1).join("\n");
  }
  const want = normal(subpath.split("#").pop() ?? subpath);
  const at = lines.findIndex((l) => {
    const m = HEADING.exec(l);
    return !!m && normal(m[2]!) === want;
  });
  if (at < 0) return body;
  const level = headingLevel(lines[at]!);
  let end = at + 1;
  while (end < lines.length) {
    const next = headingLevel(lines[end]!);
    if (next > 0 && next <= level) break;
    end += 1;
  }
  return lines.slice(at, end).join("\n");
}

/** The heading text a `[[note#Heading]]` subpath names, normalised for comparison with rendered headings. */
export const headingKey = normal;
