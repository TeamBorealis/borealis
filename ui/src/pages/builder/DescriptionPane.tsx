import { createContext, Fragment, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";
import { mdiImage } from "@mdi/js";
import { Alert, Icon, SafeHtml } from "../../components";
import { useShell } from "../../components/shell/ShellContext";
import type { Api, ElementDetail, Ruleset } from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";
import { DescriptionGrants } from "./DescriptionGrants";
import { useCopyToHomebrew } from "../homebrew/CopyToHomebrew";
import { DescriptionLinks } from "./DescriptionLinks";
import { useSettings } from "../../state/SettingsContext";

/** Read-aloud via the platform speech engine; absent engines simply disable the button. */
function useReadAloud() {
  const supported = typeof window !== "undefined" && "speechSynthesis" in window;
  const [speaking, setSpeaking] = useState(false);
  useEffect(() => () => { if (supported) window.speechSynthesis.cancel(); }, [supported]);
  const toggle = (text: string) => {
    if (!supported) return;
    if (speaking) { window.speechSynthesis.cancel(); setSpeaking(false); return; }
    const u = new SpeechSynthesisUtterance(text);
    u.onend = () => setSpeaking(false);
    u.onerror = () => setSpeaking(false);
    window.speechSynthesis.cancel();
    window.speechSynthesis.speak(u);
    setSpeaking(true);
  };
  const stop = useCallback(() => { if (supported) window.speechSynthesis.cancel(); setSpeaking(false); }, [supported]);
  return { supported, speaking, toggle, stop };
}

/**
 * True where the pane is drawn inside another landmark (the creator's phone PREVIEW sub-tab, in the editor's region): it
 * is then a named region, not an aside, since an aside belongs at the top level (H-ajc, axe
 * `landmark-complementary-is-top-level`).
 */
export const NestedPane = createContext(false);

/** The pane's landmark: an aside, or a region where it is nested ([`NestedPane`]). */
function PaneLandmark({ children }: { children?: ReactNode }) {
  const nested = useContext(NestedPane);
  return nested
    ? <section className="desc-pane" aria-label="Description">{children}</section>
    : <aside className="desc-pane" aria-label="Description">{children}</aside>;
}

/** An empty description pane (nothing described yet). */
export function EmptyPane() {
  return <PaneLandmark />;
}

/** The right-hand ElementDescriptionPanel: command icons, big uppercase title, italic intro, ruled section headings. */
export function PaneShell({ title, intro, children, readText, imageIcon = true, commands }: {
  title: ReactNode; intro?: ReactNode; children?: ReactNode; readText?: string; imageIcon?: boolean;
  /** More command icons after read-aloud (the builder's "Copy to homebrew"). */
  commands?: ReactNode;
}) {
  const voice = useReadAloud();
  const stopVoice = voice.stop;
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => { ref.current?.scrollTo?.({ top: 0 }); stopVoice(); }, [readText, stopVoice]);
  return (
    <PaneLandmark>
      <div className="desc-cmds">
        {imageIcon && <button type="button" className="desc-cmd" disabled title="No image available" aria-label="Image (none available)"><svg className="icon desc-image-icon" viewBox="3 3 18 18" preserveAspectRatio="none" aria-hidden="true" focusable="false"><path d={mdiImage} /></svg></button>}
        <button type="button" className={`desc-cmd desc-voice ${voice.speaking ? "active" : ""}`} disabled={!voice.supported || !readText}
          aria-pressed={voice.speaking} title={voice.speaking ? "Stop reading" : "Read aloud"} aria-label={voice.speaking ? "Stop reading" : "Read aloud"}
          onClick={() => readText && voice.toggle(readText)}>
          <Icon name={voice.speaking ? "stop" : "mic"} />
        </button>
        {commands}
      </div>
      <div className="desc-scroll" ref={ref} tabIndex={0}>
        <h2 className="desc-title">{title}</h2>
        {intro && <p className="desc-intro">{intro}</p>}
        {children}
      </div>
    </PaneLandmark>
  );
}

/** Text of an HTML fragment: tags become spaces, entities are decoded and runs of space collapse. */
function plainText(html: string): string {
  const spaced = html.replace(/<[^>]+>/g, " ");
  const decoded = new DOMParser().parseFromString(spaced, "text/html").documentElement.textContent ?? spaced;
  return decoded.replace(/\s+/g, " ").trim();
}

/** A trailing "..." or "…" the host adds when it cuts a short description (`elements::truncate`). */
const TRAILING_ELLIPSIS = /\s*(\.\.\.|…)$/;

/**
 * B9: the short description only repeats the body (the host cuts the body's first 160 characters and adds "..."), so the
 * pane leaves it out. An empty one counts as a repeat: there is nothing to show.
 */
export function repeatsBody(shortDescription: string, descriptionHtml: string): boolean {
  const short = shortDescription.replace(/\s+/g, " ").trim().replace(TRAILING_ELLIPSIS, "");
  return short === "" || plainText(descriptionHtml).includes(short);
}

/**
 * A3: facts that describe the host's catalogue, not the element: the type and category names are internal, and the source is
 * the pane's footer already (the printed sheet skips the same three, `sheet_api::FACT_SKIP`).
 */
const HOST_FACTS: readonly string[] = ["Type", "Source", "Category"];

/**
 * B14: the element the pane showed last, per host. Leaving Build unmounts the pane; coming back it shows this element at
 * once (while it reloads) instead of a "Loading" card.
 */
const lastShown = new WeakMap<Api, ElementDetail>();

/** Loads one element and renders it in the pane. `copyable`: the builder's pane offers "Copy to homebrew" (spec 2, 11). */
export function DescriptionPane({ elementId, fallback, copyable = false }: { elementId: string | null; fallback: ReactNode; copyable?: boolean }) {
  const api = useApi();
  const { openCompendium } = useShell();
  const copier = useCopyToHomebrew();
  const { settings } = useSettings();
  const el = useAsync(() => (elementId ? api.getElement({ elementId }) : Promise.resolve(null)), [api, elementId]);
  useEffect(() => { if (el.data) lastShown.set(api, el.data); }, [api, el.data]);
  if (!elementId) return <>{fallback}</>;
  if (el.error) return <PaneShell title="Description" imageIcon={false}><Alert tone="danger">{el.error}</Alert></PaneShell>;
  const held = lastShown.get(api);
  const d = el.data ?? (held?.id === elementId ? held : null);
  if (!d) return <PaneShell title="Loading" imageIcon={false}><p className="muted">Loading...</p></PaneShell>;
  // Rendered in place (not as a child component), so the pane stays the same element from Loading to loaded.
  const copy = copyable && copier.enabled
    ? (
      <button type="button" className="desc-cmd desc-copy" title="Copy to homebrew" aria-label="Copy to homebrew"
        onClick={() => copier.open({ id: d.id, name: d.name, isClass: d.type === "class" })}>
        <Icon name="copy" />
      </button>
    )
    : null;
  return <>{detailPane(d, openCompendium, copy, settings.linkedElements)}{copier.dialog}</>;
}

/**
 * The SOURCE line: the book and, for an element of one edition, its year ("SRD 5.2.1 (2024)"). A book name that already
 * ends with that year (a 5etools import's "Player's Handbook (2024)") is not given it twice.
 */
export function sourceLabel(sourceName: string, rulesets: readonly Ruleset[]): string {
  if (rulesets.length !== 1) return sourceName;
  const year = ` (${rulesets[0] === "dnd2024" ? "2024" : "2014"})`;
  return sourceName.endsWith(year) ? sourceName : `${sourceName}${year}`;
}

/** One element in the pane, exactly as the builder shows it (also the homebrew creator's live preview, spec 13.2). */
export function ElementDetailView({ d }: { d: ElementDetail }) {
  const { openCompendium } = useShell();
  return detailPane(d, openCompendium);
}

function detailPane(d: ElementDetail, openCompendium: ReturnType<typeof useShell>["openCompendium"], commands?: ReactNode, linkedElements = false): ReactNode {
  const dupIntro = repeatsBody(d.shortDescription, d.descriptionHtml);
  const facts = d.facts.filter((f) => !HOST_FACTS.includes(f.label));
  const plain = `${d.name}. ${d.shortDescription} ${d.descriptionHtml.replace(/<[^>]+>/g, " ")}`;
  return (
    <PaneShell title={d.name} intro={dupIntro ? undefined : d.shortDescription} readText={plain} commands={commands}>
      {d.prerequisites && <p className="desc-line"><i>Prerequisite.</i> {d.prerequisites}</p>}
      <SafeHtml html={d.descriptionHtml} />
      {facts.length > 0 && (
        <div className="desc-facts">
          {facts.map((f) => <Fragment key={f.label}><p className="desc-line"><i>{f.label}.</i> {f.value}</p></Fragment>)}
        </div>
      )}
      <DescriptionGrants grants={d.grants} />
      {linkedElements && <DescriptionLinks links={d.links ?? []} />}
      <h3 className="desc-h desc-src-h">Source</h3>
      <p className="desc-line"><button type="button" className="desc-source" onClick={() => openCompendium({ query: d.name, source: d.source })}>{sourceLabel(d.sourceName, d.rulesets)}</button></p>
    </PaneShell>
  );
}
