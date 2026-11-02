import type { ReactNode } from "react";
import { Modal, SafeHtml } from "../../components";
import { useNarrow } from "../../components/shell/useNarrow";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";
import { PaneSplit } from "../ai/Panes";

/**
 * A Play sub-tab: the list, and the description of what was tapped (GitHub #14). On wide screens the description is the
 * pane beside the list (Aurora's description pane); on phones and narrow windows it opens in a sheet over the list, which
 * Close, Escape or the back gesture dismiss.
 */
export function PlaySplit({ label, list, detail, title, onClose }: {
  label: string;
  list: ReactNode;
  /** The pane body: the selection's description, or the "select something" lead when nothing is selected. */
  detail: ReactNode;
  /** The selection's name; null when nothing is selected (no sheet on phones). */
  title: string | null;
  onClose: () => void;
}) {
  const narrow = useNarrow();
  if (!narrow) return <PaneSplit label={label} list={list} desc={detail} />;
  return (<>
    <PaneSplit label={label} list={list} />
    {title !== null && (
      <Modal title={title} onClose={onClose} className="pr-desc-modal">
        <div className="v5-desc pr-desc-body">{detail}</div>
      </Modal>
    )}
  </>);
}

/** The description of a content element (spell, weapon) in the Play pane's style, loaded from the host. */
export function ElementDescription({ elementId, lead, children }: { elementId: string | null; lead?: ReactNode; children?: ReactNode }) {
  const api = useApi();
  const el = useAsync(() => (elementId ? api.getElement({ elementId }) : Promise.resolve(null)), [api, elementId]);
  const d = el.data && el.data.id === elementId ? el.data : null;
  return (<>
    {lead}
    {children}
    {el.error && <p className="error-text">{el.error}</p>}
    {elementId && !d && !el.error && <p className="v5-note">Loading...</p>}
    {d && <SafeHtml html={d.descriptionHtml} />}
    {d && d.facts.length > 0 && <ul className="pr-facts">{d.facts.map((f) => <li key={f.label}><i>{f.label}.</i> {f.value}</li>)}</ul>}
  </>);
}
