/**
 * The notes view's reading view: markdown-it (CommonMark + GFM tables + linkify) with the Obsidian plugins, then DOMPurify.
 * Every link is rendered with `href="#"` and its target in data attributes; the view acts on clicks through backend commands
 * (open a vault note, open a web page), so nothing in a note can navigate the web view or reach files by itself.
 */
import MarkdownItFactory from "markdown-it";
import type { RendererRule, Token } from "markdown-it";
import DOMPurify from "dompurify";
import { obsidian, type CalloutMeta, type WikiLink } from "./obsidian";
import { extractSection } from "./sections";

/** What the renderer knows about the vault while it renders one note. */
export type RenderContext = {
  /** The note being rendered (links in it resolve from here). */
  from: string;
  /** Link target as written -> vault path, or null when unresolved (missing = not resolved yet). */
  resolved: ReadonlyMap<string, string | null>;
  /** Vault path -> `data:` URL of images already read. */
  images: ReadonlyMap<string, string>;
  /** Vault path -> text of notes embedded with `![[...]]`. */
  notes: ReadonlyMap<string, string>;
  /** 0 for the open note; embedded notes render at 1 and do not embed further. */
  depth: number;
};

/** The links of a note, for resolving and preloading before it is rendered. */
export interface NoteRefs {
  /** Every distinct link target (wikilinks, embeds, Markdown links and images to vault files). */
  targets: string[];
  /** Targets that are embedded (`![[x]]`, `![](x)`): images to read, notes to transclude. */
  embeds: string[];
}

const EXTERNAL = /^(?:https?:|mailto:)/i;
const IMAGE = /\.(?:png|jpe?g|gif|webp|bmp|svg|avif)$/i;
const SIZE = /^(\d+)(?:x(\d+))?$/;

const escapeHtml = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const fileName = (p: string) => p.split("/").pop() ?? p;
const stripMd = (p: string) => p.replace(/\.md$/i, "");
function decode(s: string): string {
  try { return decodeURIComponent(s); } catch { return s; }
}
/** A relative Markdown link destination as a vault target and subpath. */
function splitHref(href: string): { target: string; subpath: string | null } {
  const decoded = decode(href);
  const hash = decoded.indexOf("#");
  return hash < 0 ? { target: decoded, subpath: null } : { target: decoded.slice(0, hash), subpath: decoded.slice(hash + 1) || null };
}
const capitalise = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

function linkAttrs(target: string, subpath: string | null, ctx: RenderContext): string {
  const resolved = target ? ctx.resolved.get(target) : ctx.from;
  const cls = resolved === null ? "md-wikilink is-unresolved" : "md-wikilink";
  return `href="#" class="${cls}" data-wikilink="${escapeHtml(target)}" data-subpath="${escapeHtml(subpath ?? "")}" data-from="${escapeHtml(ctx.from)}"`;
}

function wikiLinkHtml(link: WikiLink, ctx: RenderContext): string {
  const shown = link.alias ?? (link.target ? (link.subpath ? `${stripMd(link.target)} > ${link.subpath.replace(/^\^/, "")}` : stripMd(link.target)) : (link.subpath ?? "").replace(/^\^/, ""));
  return `<a ${linkAttrs(link.target, link.subpath, ctx)}>${escapeHtml(shown)}</a>`;
}

function embedHtml(link: WikiLink, ctx: RenderContext): string {
  const path = ctx.resolved.get(link.target);
  if (!path) return path === null ? `<span class="md-embed is-unresolved">${wikiLinkHtml({ ...link, embed: false }, ctx)}</span>` : wikiLinkHtml(link, ctx);
  if (IMAGE.test(path)) {
    const url = ctx.images.get(path);
    if (!url) return `<span class="md-embed-loading">${escapeHtml(fileName(path))}</span>`;
    const size = link.alias ? SIZE.exec(link.alias) : null;
    const dims = size ? ` width="${size[1]}"${size[2] ? ` height="${size[2]}"` : ""}` : "";
    const alt = link.alias && !size ? link.alias : fileName(path);
    return `<img class="md-embed-image" src="${escapeHtml(url)}" alt="${escapeHtml(alt)}"${dims}>`;
  }
  const text = ctx.notes.get(path);
  if (ctx.depth > 0 || text === undefined || !/\.md$/i.test(path)) return wikiLinkHtml(link, ctx);
  const inner = renderRaw(extractSection(text, link.subpath), { ...ctx, from: path, depth: ctx.depth + 1 });
  const title = link.subpath ? `${stripMd(fileName(path))} > ${link.subpath.replace(/^\^/, "")}` : stripMd(fileName(path));
  return `<div class="md-embed" data-embed="${escapeHtml(path)}"><div class="md-embed-title"><a ${linkAttrs(link.target, link.subpath, ctx)}>${escapeHtml(title)}</a></div><div class="md-embed-body">${inner}</div></div>`;
}

/** The render context markdown-it hands back to each rule. */
const ctxOf = (env: unknown): RenderContext => env as RenderContext;
/** A token attribute as text. */
const attr = (t: Token, name: string): string => String(t.attrGet(name) ?? "");

function rules(): Record<string, RendererRule> {
  return {
    wikilink: (tokens, idx, _o, env) => {
      const link = tokens[idx]!.meta as unknown as WikiLink;
      return link.embed ? embedHtml(link, ctxOf(env)) : wikiLinkHtml(link, ctxOf(env));
    },
    tag: (tokens, idx) => {
      const tag = tokens[idx]!.content;
      return `<a href="#" class="tag" data-tag="${escapeHtml(tag)}">#${escapeHtml(tag)}</a>`;
    },
    link_open: (tokens, idx, options, envIn, self) => {
      const env = ctxOf(envIn);
      const token = tokens[idx]!;
      const href = attr(token, "href");
      if (EXTERNAL.test(href)) {
        token.attrs = [["href", "#"], ["class", "md-external"], ["data-external", href], ["title", href]];
      } else {
        const { target, subpath } = href.startsWith("#") ? { target: "", subpath: decode(href.slice(1)) || null } : splitHref(href);
        const resolved = target ? env.resolved.get(target) : env.from;
        token.attrs = [["href", "#"], ["class", resolved === null ? "md-wikilink is-unresolved" : "md-wikilink"], ["data-wikilink", target], ["data-subpath", subpath ?? ""], ["data-from", env.from]];
      }
      return self.renderToken(tokens, idx, options);
    },
    image: (tokens, idx, _o, env) => {
      const token = tokens[idx]!;
      const src = attr(token, "src");
      const alt = token.content;
      if (EXTERNAL.test(src)) return `<a href="#" class="md-external" data-external="${escapeHtml(src)}" title="${escapeHtml(src)}">${escapeHtml(alt || src)}</a>`;
      const { target } = splitHref(src);
      return embedHtml({ embed: true, target, subpath: null, alias: alt || null }, ctxOf(env));
    },
    heading_open: (tokens, idx, options, _env, self) => {
      tokens[idx]!.attrSet("data-heading", tokens[idx + 1]?.content ?? "");
      return self.renderToken(tokens, idx, options);
    },
    blockquote_open: (tokens, idx, options, _env, self) => {
      const meta = tokens[idx]!.meta as unknown as CalloutMeta | null;
      if (!meta?.callout) return self.renderToken(tokens, idx, options);
      const title = escapeHtml(meta.title || capitalise(meta.callout));
      const kind = escapeHtml(meta.callout);
      if (meta.fold) return `<details class="callout" data-callout="${kind}"${meta.fold === "+" ? " open" : ""}><summary class="callout-title">${title}</summary><div class="callout-content">\n`;
      return `<div class="callout" data-callout="${kind}"><div class="callout-title">${title}</div><div class="callout-content">\n`;
    },
    blockquote_close: (tokens, idx, options, _env, self) => {
      const meta = tokens[idx]!.meta as unknown as CalloutMeta | null;
      if (!meta?.callout) return self.renderToken(tokens, idx, options);
      return meta.fold ? "</div></details>\n" : "</div></div>\n";
    },
  };
}

/** Table cell alignment as a class (inline styles are not allowed through the sanitiser). */
function alignCells(md: ReturnType<typeof MarkdownItFactory>): void {
  for (const type of ["th_open", "td_open"]) {
    md.renderer.rules[type] = (tokens, idx, options, _env, self) => {
      const token = tokens[idx]!;
      const align = /text-align:(\w+)/.exec(attr(token, "style"))?.[1];
      token.attrs = align ? [["class", `align-${align}`]] : null;
      return self.renderToken(tokens, idx, options);
    };
  }
}

let parser: ReturnType<typeof MarkdownItFactory> | null = null;
function md(): ReturnType<typeof MarkdownItFactory> {
  if (parser) return parser;
  const next = MarkdownItFactory({ html: true, linkify: true, breaks: false, typographer: false });
  next.use(obsidian);
  Object.assign(next.renderer.rules, rules());
  alignCells(next);
  parser = next;
  return next;
}

function renderRaw(text: string, ctx: RenderContext): string {
  return md().render(text, ctx as unknown as Record<string, unknown>);
}

/**
 * Attributes whose values are not addresses. DOMPurify checks every other allowed attribute against `ALLOWED_URI_REGEXP`
 * (here: `#` only), so only `href` and `src` are held to it (`src` may also be a `data:` image).
 */
const URI_SAFE = [
  "data-wikilink", "data-subpath", "data-from", "data-tag", "data-task-line", "data-external", "data-heading", "data-block", "data-callout",
  "data-embed", "width", "height", "type", "checked", "disabled", "open", "colspan", "rowspan", "start",
];
const ALLOWED_TAGS = [
  "a", "p", "br", "hr", "h1", "h2", "h3", "h4", "h5", "h6", "strong", "em", "b", "i", "u", "s", "del", "ins", "mark", "sub", "sup",
  "small", "abbr", "code", "pre", "kbd", "blockquote", "ul", "ol", "li", "table", "thead", "tbody", "tr", "th", "td", "img", "input",
  "details", "summary", "div", "span",
];
const ALLOWED_ATTR = ["class", "title", "alt", "src", "width", "height", "type", "checked", "open", "colspan", "rowspan", "start", "href", ...URI_SAFE];

let purifier: ReturnType<typeof DOMPurify> | null = null;
function purify(): ReturnType<typeof DOMPurify> {
  if (purifier) return purifier;
  const p = DOMPurify(window);
  // A raw HTML web link in a note keeps working the same way as a Markdown one: the address moves to data-external.
  p.addHook("uponSanitizeAttribute", (node, data) => {
    if (data.attrName === "href" && node.tagName === "A" && EXTERNAL.test(data.attrValue)) {
      node.setAttribute("data-external", data.attrValue);
      data.attrValue = "#";
    }
  });
  p.addHook("afterSanitizeAttributes", (node) => {
    if (node.tagName === "INPUT" && node.getAttribute("type") !== "checkbox") node.remove();
    if (node.tagName === "A") node.setAttribute("href", "#");
  });
  purifier = p;
  return p;
}

/** Sanitise rendered HTML: an allow-list of tags and attributes, `#` links only, images only as `data:` URLs. */
export function sanitizeNoteHtml(html: string): string {
  return purify().sanitize(html, {
    ALLOWED_TAGS, ALLOWED_ATTR, ADD_URI_SAFE_ATTR: URI_SAFE, ALLOW_DATA_ATTR: false, ALLOWED_URI_REGEXP: /^#/,
  });
}

/** Render a note's text (LF) to sanitised HTML. */
export function renderNote(text: string, ctx: RenderContext): string {
  return sanitizeNoteHtml(renderRaw(text, ctx));
}

function walk(tokens: Token[], visit: (t: Token) => void): void {
  for (const t of tokens) {
    visit(t);
    if (t.children) walk(t.children, visit);
  }
}

/** The link targets of a note (for one `vaultResolve` call before rendering). */
export function noteRefs(text: string): NoteRefs {
  const targets = new Set<string>();
  const embeds = new Set<string>();
  walk(md().parse(text, {}), (t) => {
    if (t.type === "wikilink") {
      const link = t.meta as unknown as WikiLink;
      if (!link.target) return;
      targets.add(link.target);
      if (link.embed) embeds.add(link.target);
    } else if (t.type === "link_open" || t.type === "image") {
      const href = attr(t, t.type === "image" ? "src" : "href");
      if (!href || EXTERNAL.test(href) || href.startsWith("#")) return;
      const { target } = splitHref(href);
      if (!target) return;
      targets.add(target);
      if (t.type === "image") embeds.add(target);
    }
  });
  return { targets: [...targets], embeds: [...embeds] };
}
