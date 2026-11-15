import { useState } from "react";
import type { HomebrewDataNode, HomebrewGeneratedView } from "../../api/contract";
import { Icon, SafeHtml } from "../../components";
import { ToggleSwitch } from "../../components/controls";
import { noteStatus } from "./statusNote";

/** One line of the tree: `key: value`, an HTML description rendered (or as its text), an expression beside its sentence. */
function Line({ node, depth, showHtml }: { node: HomebrewDataNode; depth: number; showHtml: boolean }) {
  const branch = node.kind === "object" || node.kind === "list";
  const value = node.kind === "html" && !showHtml
    ? <span className="hb-data-html"><SafeHtml html={node.text ?? ""} /></span>
    : <span className={`hb-data-val hb-data-${node.kind}`}>{node.kind === "empty" ? "(none)" : node.text}</span>;
  return (
    <li role="treeitem" aria-level={depth + 1} aria-expanded={branch ? true : undefined} aria-label={branch ? node.key : `${node.key}: ${node.text ?? ""}`}>
      <div className="hb-data-line" style={{ paddingLeft: depth * 14 }}>
        <span className="hb-data-key">{node.key}{branch ? "" : ":"}</span>
        {!branch && value}
        {node.sentence && <span className="hb-data-sentence">{node.sentence}</span>}
      </div>
      {branch && (node.children?.length ?? 0) > 0 && (
        <ul role="group">{node.children!.map((c, i) => <Line key={`${c.key}-${i}`} node={c} depth={depth + 1} showHtml={showHtml} />)}</ul>
      )}
    </li>
  );
}

/**
 * The DATA sub-tab (spec 12.1, 13.14): the element as it is stored, as an indented `key: value` tree on the description
 * pane's `#101010`. Strings are their characters, never escaped; a description is shown rendered, or as its HTML with "Show
 * HTML"; expressions sit beside their sentence. "Copy as JSON" copies the element as the content file writes it (for bug
 * reports and other tools). Nothing here is editable.
 */
export function DataView({ generated }: { generated: HomebrewGeneratedView | null | undefined }) {
  const [showHtml, setShowHtml] = useState(false);
  const tree = generated?.tree ?? [];
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(generated?.json ?? "");
      noteStatus("Copied the element as JSON.");
    } catch {
      noteStatus("The clipboard can't be reached here.");
    }
  };
  return (
    <section className="hb-data" aria-label="Generated data">
      <div className="eq-inv-actions hb-data-tools" role="toolbar" aria-label="Generated data commands">
        <button type="button" disabled={!generated?.json} onClick={() => void copy()} title="Copy the element as JSON"><Icon name="copy" />Copy as JSON</button>
        <span className="hb-expert"><ToggleSwitch label="Show HTML" checked={showHtml} onChange={setShowHtml} onLabel="Show HTML" offLabel="Show HTML" /></span>
      </div>
      <p className="hb-field-help">The element as it is stored. Read only: change it in the other sections.</p>
      {tree.length === 0
        ? <p className="hb-pending">Nothing to show until the form can make an element.</p>
        : <ul className="hb-data-tree" role="tree" aria-label="Stored element">{tree.map((n, i) => <Line key={`${n.key}-${i}`} node={n} depth={0} showHtml={showHtml} />)}</ul>}
    </section>
  );
}
