/**
 * Obsidian's syntax as markdown-it plugins: front matter, `[[wikilinks]]` / `![[embeds]]`, `#tags`, callouts
 * (`> [!note]`, foldable with `+` / `-`), task lists and `^block` ids. markdown-it parses CommonMark and GFM tables; these
 * rules only add the extra syntax, with the same rules as the backend (`src-tauri/src/vault/syntax.rs`).
 */
import type { MarkdownIt, StateBlock, StateCore, StateInline, Token } from "markdown-it";
import { splitFrontMatter } from "./frontMatter";

/** A parsed `[[target#subpath|alias]]`. */
export type WikiLink = {
  embed: boolean;
  /** The file part (empty for a link to a heading of the same note). */
  target: string;
  subpath: string | null;
  alias: string | null;
};

/** What a callout's blockquote tokens carry in `meta`. */
export type CalloutMeta = {
  callout: string;
  /** "" = not foldable, "+" = foldable and open, "-" = foldable and closed. */
  fold: "" | "+" | "-";
  title: string;
};

const TAG_BODY = /^#([\p{L}\p{N}\p{M}_/-]*[\p{L}\p{M}_/-][\p{L}\p{N}\p{M}_/-]*)/u;
const CALLOUT_HEAD = /^\[!([^\]\s]+)\]([+-]?)[ \t]*(.*)$/;
const BLOCK_ID = /\s\^([A-Za-z0-9-]+)\s*$/;
const TASK_HEAD = /^\[(.)\](?=\s|$)\s?/;

/** Split the inside of `[[...]]` like the backend: `\|` (in tables) also separates the alias. */
export function parseWikiLink(inner: string, embed: boolean): WikiLink {
  const pipe = inner.indexOf("|");
  let link = pipe < 0 ? inner : inner.slice(0, pipe);
  const alias = pipe < 0 ? null : inner.slice(pipe + 1).trim() || null;
  if (link.endsWith("\\")) link = link.slice(0, -1);
  const hash = link.indexOf("#");
  const target = (hash < 0 ? link : link.slice(0, hash)).trim();
  const subpath = hash < 0 ? null : link.slice(hash + 1).trim() || null;
  return { embed, target, subpath, alias };
}

function frontMatterBlock(state: StateBlock, startLine: number, _endLine: number, silent: boolean): boolean {
  if (startLine !== 0 || state.blkIndent !== 0) return false;
  const fm = splitFrontMatter(state.src);
  if (!fm) return false;
  if (silent) return true;
  const token = state.push("front_matter", "", 0);
  token.hidden = true;
  token.content = fm.yaml;
  token.map = [0, fm.endLine];
  state.line = fm.endLine;
  return true;
}

function wikiLinkInline(state: StateInline, silent: boolean): boolean {
  const { src } = state;
  const embed = src.charCodeAt(state.pos) === 0x21;
  const open = state.pos + (embed ? 1 : 0);
  if (src.charCodeAt(open) !== 0x5b || src.charCodeAt(open + 1) !== 0x5b) return false;
  const close = src.indexOf("]]", open + 2);
  if (close < 0 || close + 2 > state.posMax) return false;
  const inner = src.slice(open + 2, close);
  if (!inner || /[[\]\n]/.test(inner)) return false;
  if (!silent) {
    const token = state.push("wikilink", "", 0);
    token.meta = parseWikiLink(inner, embed);
    token.content = src.slice(state.pos, close + 2);
  }
  state.pos = close + 2;
  return true;
}

function tagInline(state: StateInline, silent: boolean): boolean {
  if (state.src.charCodeAt(state.pos) !== 0x23) return false;
  if (state.pos > 0 && !/\s/.test(state.src[state.pos - 1]!)) return false;
  const m = TAG_BODY.exec(state.src.slice(state.pos, state.posMax));
  if (!m) return false;
  if (!silent) {
    const token = state.push("tag", "", 0);
    token.content = m[1]!.replace(/\/+$/, "");
  }
  state.pos += m[0].length;
  return true;
}

/** The blockquote close that matches the open at `i`. */
function matchingClose(tokens: Token[], i: number): number {
  let depth = 0;
  for (let j = i; j < tokens.length; j += 1) {
    if (tokens[j]!.type === "blockquote_open") depth += 1;
    else if (tokens[j]!.type === "blockquote_close" && --depth === 0) return j;
  }
  return -1;
}

/** `> [!type]+ Title` blockquotes become callouts (before inline parsing, so the title line is taken out of the text). */
function callouts(state: StateCore): void {
  const tokens = state.tokens;
  for (let i = 0; i < tokens.length; i += 1) {
    const open = tokens[i]!;
    const inline = tokens[i + 2];
    if (open.type !== "blockquote_open" || tokens[i + 1]?.type !== "paragraph_open" || inline?.type !== "inline") continue;
    const newline = inline.content.indexOf("\n");
    const m = CALLOUT_HEAD.exec(newline < 0 ? inline.content : inline.content.slice(0, newline));
    const close = matchingClose(tokens, i);
    if (!m || close < 0) continue;
    const meta: CalloutMeta = { callout: m[1]!.toLowerCase(), fold: m[2] as CalloutMeta["fold"], title: m[3]!.trim() };
    open.meta = meta;
    tokens[close]!.meta = meta;
    if (newline < 0) tokens.splice(i + 1, 3);
    else inline.content = inline.content.slice(newline + 1);
  }
}

/**
 * `- [ ] task` list items get a checkbox that knows its source line; `^id` block markers are hidden and kept as ids. In an
 * embedded note (`env.depth` > 0) the checkboxes are disabled: their lines belong to the other note.
 */
function tasksAndBlocks(state: StateCore): void {
  const tokens = state.tokens;
  const embedded = Number((state.env as { depth?: number } | undefined)?.depth ?? 0) > 0;
  for (let i = 0; i < tokens.length; i += 1) {
    const inline = tokens[i]!;
    if (inline.type !== "inline" || !inline.children?.length) continue;
    const last = inline.children[inline.children.length - 1]!;
    const id = last.type === "text" ? BLOCK_ID.exec(last.content) : null;
    if (id) {
      last.content = last.content.slice(0, id.index);
      tokens[i - 1]?.attrSet("data-block", id[1]!);
    }
    const item = tokens[i - 2];
    const first = inline.children[0]!;
    const task = first.type === "text" ? TASK_HEAD.exec(first.content) : null;
    if (item?.type !== "list_item_open" || tokens[i - 1]?.type !== "paragraph_open" || !task || !item.map) continue;
    first.content = first.content.slice(task[0].length);
    const box = new state.Token("html_inline", "", 0);
    const checked = task[1] !== " ";
    const line = embedded ? " disabled" : ` data-task-line="${item.map[0]}"`;
    box.content = `<input type="checkbox" class="task-checkbox"${line}${checked ? " checked" : ""}> `;
    inline.children.unshift(box);
    item.attrJoin("class", checked ? "task-list-item is-checked" : "task-list-item");
  }
}

/** Register the Obsidian syntax on `md`. */
export function obsidian(md: MarkdownIt): void {
  md.block.ruler.before("table", "front_matter", frontMatterBlock);
  md.inline.ruler.before("link", "wikilink", wikiLinkInline);
  md.inline.ruler.before("emphasis", "tag", tagInline);
  md.core.ruler.before("inline", "callouts", callouts);
  md.core.ruler.after("inline", "tasks_blocks", tasksAndBlocks);
  md.renderer.rules.front_matter = () => "";
}
