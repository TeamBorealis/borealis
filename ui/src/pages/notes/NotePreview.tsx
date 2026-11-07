import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import { parseProperties, propertyTags, splitFrontMatter } from "./markdown/frontMatter";
import { noteRefs, renderNote } from "./markdown/render";
import { headingKey } from "./markdown/sections";

/** A link the user clicked in a note. */
export interface ClickedLink {
  /** The note the link is written in (links resolve from there). */
  from: string;
  target: string;
  subpath: string | null;
}

interface PreviewProps {
  path: string;
  /** The note text (LF). */
  text: string;
  /** Bumped when the vault changes on disk, so embeds are read again. */
  version: number;
  /** Heading or `^block` to scroll to once rendered. */
  scrollTo: string | null;
  onLink: (link: ClickedLink) => void;
  onTag: (tag: string) => void;
  /** Toggle the task on this 0-based source line (read-only notes pass null). */
  onToggleTask: ((line: number) => void) | null;
}

/** Most embeds read for one note. */
const MAX_EMBEDS = 40;
const IMAGE = /\.(?:png|jpe?g|gif|webp|bmp|svg|avif)$/i;
const WIKI_VALUE = /^\[\[([^[\]|#]+)(?:#([^[\]|]+))?(?:\|[^[\]]+)?\]\]$/;

interface Loaded {
  resolved: ReadonlyMap<string, string | null>;
  images: ReadonlyMap<string, string>;
  notes: ReadonlyMap<string, string>;
}
const NOTHING: Loaded = { resolved: new Map(), images: new Map(), notes: new Map() };

function PropertyValue({ name, value, path, onTag, onLink }: { name: string; value: unknown; path: string } & Pick<PreviewProps, "onTag" | "onLink">) {
  if (name === "tags" || name === "tag") {
    return <>{propertyTags(value).map((t) => <button key={t} type="button" className="nt-chip tag" onClick={() => onTag(t)}>#{t}</button>)}</>;
  }
  const items = Array.isArray(value) ? value : [value];
  return (
    <>
      {items.map((item, i) => {
        const text = item === null || item === undefined ? "" : typeof item === "object" ? JSON.stringify(item) : String(item);
        const link = WIKI_VALUE.exec(text);
        return link
          ? <button key={i} type="button" className="nt-chip md-wikilink" onClick={() => onLink({ from: path, target: link[1]!.trim(), subpath: link[2] ?? null })}>{link[1]}</button>
          : <span key={i} className="nt-chip">{text}</span>;
      })}
    </>
  );
}

/** Front matter shown as properties, the way Obsidian's reading view does. */
function NoteProperties({ text, path, onTag, onLink }: Pick<PreviewProps, "text" | "path" | "onTag" | "onLink">) {
  const fm = useMemo(() => splitFrontMatter(text), [text]);
  const props = useMemo(() => (fm ? parseProperties(fm.yaml) : null), [fm]);
  if (!fm || !props) return null;
  return (
    <section className="nt-props" aria-label="Properties">
      <h3 className="nt-props-title">Properties</h3>
      {props.error && <p className="nt-props-error">{props.error}</p>}
      <dl>
        {props.entries.map(([name, value]) => (
          <div key={name} className="nt-prop">
            <dt>{name}</dt>
            <dd><PropertyValue name={name} value={value} path={path} onTag={onTag} onLink={onLink} /></dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

/** Resolve the note's links and read its embeds (images and one level of notes) through the backend. */
function useLoaded(path: string, text: string, version: number): Loaded {
  const api = useApi();
  const refs = useMemo(() => noteRefs(text), [text]);
  const [loaded, setLoaded] = useState<Loaded>(NOTHING);
  const cache = useRef<{ version: number; files: Map<string, string> }>({ version, files: new Map() });
  if (cache.current.version !== version) cache.current = { version, files: new Map() };
  const key = refs.targets.join("\n");
  useEffect(() => {
    let alive = true;
    const files = cache.current.files;
    void (async () => {
      const list = refs.targets.length ? await api.vaultResolve({ from: path, targets: refs.targets }) : [];
      const resolved = new Map(refs.targets.map((t, i) => [t, list[i] ?? null]));
      if (!alive) return;
      setLoaded((cur) => ({ ...cur, resolved }));
      const wanted = [...new Set(refs.embeds.map((t) => resolved.get(t)).filter((p): p is string => !!p && p !== path))].slice(0, MAX_EMBEDS);
      await Promise.all(wanted.filter((p) => !files.has(p)).map(async (p) => {
        try {
          if (IMAGE.test(p)) files.set(p, await api.vaultImage({ path: p }));
          else if (/\.md$/i.test(p)) files.set(p, (await api.vaultRead({ path: p })).text.replace(/\r\n/g, "\n"));
        } catch { /* an embed that cannot be read stays a link */ }
      }));
      if (!alive) return;
      const images = new Map(wanted.filter((p) => IMAGE.test(p) && files.has(p)).map((p) => [p, files.get(p)!]));
      const notes = new Map(wanted.filter((p) => !IMAGE.test(p) && files.has(p)).map((p) => [p, files.get(p)!]));
      setLoaded({ resolved, images, notes });
    })().catch((e) => alive && setStatus(errorMessage(e)));
    return () => { alive = false; };
    // `key` stands for refs.targets; refs.embeds follows from the same text.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, path, key, version]);
  return loaded;
}

/** The reading view: the rendered, sanitised note. Clicks act through the callbacks (and the backend for web links). */
export function NotePreview({ path, text, version, scrollTo, onLink, onTag, onToggleTask }: PreviewProps) {
  const api = useApi();
  const loaded = useLoaded(path, text, version);
  const html = useMemo(() => renderNote(text, { from: path, depth: 0, ...loaded }), [text, path, loaded]);
  const body = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!scrollTo || !body.current) return;
    const nodes = scrollTo.startsWith("^")
      ? [...body.current.querySelectorAll<HTMLElement>("[data-block]")].filter((n) => n.dataset.block === scrollTo.slice(1))
      : [...body.current.querySelectorAll<HTMLElement>("[data-heading]")].filter((n) => headingKey(n.dataset.heading ?? "") === headingKey(scrollTo));
    nodes[0]?.scrollIntoView?.({ block: "start" });
  }, [scrollTo, html]);

  const onClick = (e: MouseEvent<HTMLDivElement>) => {
    const el = e.target as HTMLElement;
    const box = el.closest<HTMLInputElement>("input.task-checkbox");
    if (box) {
      e.preventDefault();
      // A task in an embedded note belongs to that note: never toggle a line of this one for it.
      const line = Number(box.dataset.taskLine);
      if (onToggleTask && !box.closest(".md-embed") && box.dataset.taskLine && Number.isInteger(line)) onToggleTask(line);
      return;
    }
    const a = el.closest<HTMLAnchorElement>("a");
    if (!a) return;
    e.preventDefault();
    if (a.dataset.external) void api.vaultOpenUrl({ url: a.dataset.external }).catch((err) => setStatus(errorMessage(err)));
    else if (a.dataset.tag) onTag(a.dataset.tag);
    else if (a.dataset.wikilink !== undefined) onLink({ from: a.dataset.from || path, target: a.dataset.wikilink, subpath: a.dataset.subpath || null });
  };

  return (
    <article className="nt-read" aria-label="Reading view">
      <NoteProperties text={text} path={path} onTag={onTag} onLink={onLink} />
      <div ref={body} className={`md-preview${onToggleTask ? "" : " read-only"}`} onClick={onClick} dangerouslySetInnerHTML={{ __html: html }} />
    </article>
  );
}
