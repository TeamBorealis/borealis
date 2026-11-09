import { Fragment, type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { mdiImage } from "@mdi/js";
import { Alert, Icon, SafeHtml } from "../../components";
import { useShell } from "../../components/shell/ShellContext";
import type { Api, ElementDetail } from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";
import { DescriptionGrants } from "./DescriptionGrants";

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

/** The right-hand ElementDescriptionPanel: command icons, big uppercase title, italic intro, ruled section headings. */
export function PaneShell({ title, intro, children, readText, imageIcon = true }: {
  title: ReactNode; intro?: ReactNode; children?: ReactNode; readText?: string; imageIcon?: boolean;
}) {
  const voice = useReadAloud();
  const stopVoice = voice.stop;
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => { ref.current?.scrollTo?.({ top: 0 }); stopVoice(); }, [readText, stopVoice]);
  return (
    <aside className="desc-pane" aria-label="Description">
      <div className="desc-cmds">
        {imageIcon && <button type="button" className="desc-cmd" disabled title="No image available" aria-label="Image (none available)"><svg className="icon desc-image-icon" viewBox="3 3 18 18" preserveAspectRatio="none" aria-hidden="true" focusable="false"><path d={mdiImage} /></svg></button>}
        <button type="button" className={`desc-cmd desc-voice ${voice.speaking ? "active" : ""}`} disabled={!voice.supported || !readText}
          aria-pressed={voice.speaking} title={voice.speaking ? "Stop reading" : "Read aloud"} aria-label={voice.speaking ? "Stop reading" : "Read aloud"}
          onClick={() => readText && voice.toggle(readText)}>
          <Icon name={voice.speaking ? "stop" : "mic"} />
        </button>
      </div>
      <div className="desc-scroll" ref={ref} tabIndex={0}>
        <h2 className="desc-title">{title}</h2>
        {intro && <p className="desc-intro">{intro}</p>}
        {children}
      </div>
    </aside>
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
 * B14: the element the pane showed last, per host. Leaving Build unmounts the pane; coming back it shows this element at
 * once (while it reloads) instead of a "Loading" card.
 */
const lastShown = new WeakMap<Api, ElementDetail>();

/** Loads one element and renders it in the pane. */
export function DescriptionPane({ elementId, fallback }: { elementId: string | null; fallback: ReactNode }) {
  const api = useApi();
  const { openCompendium } = useShell();
  const el = useAsync(() => (elementId ? api.getElement({ elementId }) : Promise.resolve(null)), [api, elementId]);
  useEffect(() => { if (el.data) lastShown.set(api, el.data); }, [api, el.data]);
  if (!elementId) return <>{fallback}</>;
  if (el.error) return <PaneShell title="Description" imageIcon={false}><Alert tone="danger">{el.error}</Alert></PaneShell>;
  const held = lastShown.get(api);
  const d = el.data ?? (held?.id === elementId ? held : null);
  if (!d) return <PaneShell title="Loading" imageIcon={false}><p className="muted">Loading...</p></PaneShell>;
  const dupIntro = repeatsBody(d.shortDescription, d.descriptionHtml);
  const plain = `${d.name}. ${d.shortDescription} ${d.descriptionHtml.replace(/<[^>]+>/g, " ")}`;
  return (
    <PaneShell title={d.name} intro={dupIntro ? undefined : d.shortDescription} readText={plain}>
      {d.prerequisites && <p className="desc-line"><i>Prerequisite.</i> {d.prerequisites}</p>}
      <SafeHtml html={d.descriptionHtml} />
      {d.facts.length > 0 && (
        <div className="desc-facts">
          {d.facts.map((f) => <Fragment key={f.label}><p className="desc-line"><i>{f.label}.</i> {f.value}</p></Fragment>)}
        </div>
      )}
      <DescriptionGrants grants={d.grants} />
      <h3 className="desc-h desc-src-h">Source</h3>
      <p className="desc-line"><button type="button" className="desc-source" onClick={() => openCompendium({ query: d.name, source: d.source })}>{d.sourceName}{d.rulesets.length === 1 ? ` (${d.rulesets[0] === "dnd2024" ? "2024" : "2014"})` : ""}</button></p>
    </PaneShell>
  );
}
