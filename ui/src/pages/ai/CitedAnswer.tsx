import { Fragment } from "react";
import type { CitedAnswerData, CitedSource } from "../../api/contract";
import { Tag, type TagTone } from "./Panes";
import { splitCitations } from "./citations";
import { Markdown } from "./markdown";

export type OpenSource = (source: CitedSource) => void;

/**
 * Answer text with its `[[ELEMENT_ID]]` markers rendered as source links (the classic builder's
 * blue underlined italic hyperlink). Markers with no resolved source (still streaming, or
 * stripped by validation) are dropped.
 */
export function CitedText({ text, sources, onOpen }: { text: string; sources?: CitedSource[]; onOpen: OpenSource }) {
  const byId = new Map((sources ?? []).map((s) => [s.id, s]));
  const renderText = (chunk: string, key: string) => (
    <Fragment key={key}>
      {splitCitations(chunk).map((p, i) => {
        if (p.kind === "text") return p.text;
        const s = byId.get(p.id);
        return s ? (
          <button key={i} type="button" className="v5-cite" title={`${s.name} (${s.type})`} aria-label={`Citation: ${s.name}`} onClick={() => onOpen(s)}>{s.name}</button>
        ) : null;
      })}
    </Fragment>
  );
  return <Markdown text={text} renderText={renderText} />;
}

const CONFIDENCE_TONE: Record<CitedAnswerData["confidence"], TagTone> = { high: "ok", medium: "warn", low: "bad" };

/** Citation chips and the validation verdict under a Rules Q&A answer. */
export function CitedSources({ data, onOpen }: { data: CitedAnswerData; onOpen: OpenSource }) {
  return (
    <div className="v5-cited" aria-label="Answer sources">
      {data.unverified && <p className="v5-note warn">Unverified: no citation to your enabled content supports this answer.</p>}
      {data.notCovered && <p className="v5-note">This question is not covered by your enabled content.</p>}
      {data.sources.length > 0 && (
        <>
          <div className="v5-group">Sources</div>
          <ul className="v5-chips" aria-label="Citations">
            {data.sources.map((s) => (
              <li key={s.id}>
                <button type="button" className="v5-chip" title={`Open ${s.name} in the compendium`} onClick={() => onOpen(s)}>
                  <span className="n">{s.name}</span><span className="t">{s.type}</span>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
      <Tag tone={CONFIDENCE_TONE[data.confidence]} title="Confidence after checking the citations">{`${data.confidence} confidence`}</Tag>
    </div>
  );
}
