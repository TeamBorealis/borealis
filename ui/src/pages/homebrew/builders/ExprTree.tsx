import type { ReactNode } from "react";
import type { HomebrewBuilderProblem, HomebrewExprNode, HomebrewGroupMode } from "../../../api/contract";
import { ComboBox, Icon } from "../../../components";
import { useCreatorEnv } from "../env";
import { GROUP_MODES, problemsAt } from "./words";

/** Groups nest at most this deep: deeper trees read badly and no bundled expression needs them. */
const MAX_DEPTH = 3;

export interface RowEditor<R> {
  /** The row's controls. */
  render: (row: R, onRow: (row: R) => void, label: string) => ReactNode;
  /** A new row, for Add. */
  make: () => R;
  /** A row the builder can't represent: read-only, removable (hc-wb02 `Kept`). */
  kept: (row: R) => string | null;
  /** "Add condition", "Add tag". */
  addLabel: string;
}

export interface ExprTreeProps<R> {
  /** Shown name of the whole tree ("Prerequisite"), for accessible names. */
  label: string;
  node: Extract<HomebrewExprNode<R>, { node: "group" }>;
  onChange: (node: Extract<HomebrewExprNode<R>, { node: "group" }>) => void;
  editor: RowEditor<R>;
  problems: readonly HomebrewBuilderProblem[];
  /** The group's path in the model (`` for the root, `items.2` for a nested group). */
  path: string;
  depth?: number;
  /** The root can't be removed; nested groups can. */
  onRemove?: () => void;
  /** Limit the modes (a Specific tag filter is always Any of). */
  modes?: readonly HomebrewGroupMode[];
}

const join = (path: string, more: string): string => (path ? `${path}.${more}` : more);

/** Problem sentences under a row or group (spec 8.3: linked to the field, never colour alone). */
function RowProblems({ problems }: { problems: readonly HomebrewBuilderProblem[] }) {
  if (problems.length === 0) return null;
  return <div className="hb-row-msg" role="note">{problems.map((p, i) => <p key={i}><Icon name="x" />{p.message}</p>)}</div>;
}

/** A "kept as is" part (spec 6.5): read-only, with Remove; experts see the stored text. */
export function KeptChip({ text, reason, onRemove, what = "part" }: { text: string; reason?: string; onRemove: () => void; what?: string }) {
  const { expert } = useCreatorEnv();
  return (
    <span className="hb-kept">
      <span className="hb-kept-label">Kept as is</span>
      <span className="hb-kept-text">{reason ?? `Borealis keeps this ${what} unchanged; it can only be removed.`}{expert && <code> {text}</code>}</span>
      <button type="button" className="hb-chip-x" aria-label={`Remove the kept ${what}`} onClick={onRemove}><Icon name="x" /></button>
    </span>
  );
}

/**
 * The All of / Any of / None of tree every boolean builder shares (spec 6.3; hc-wb02 `HomebrewExprNode`): a mode combo,
 * the rows (each with Remove), nested groups, and Add buttons. A row's problems (paths `items.N...`) show under it.
 */
export function ExprTree<R>({ label, node, onChange, editor, problems, path, depth = 0, onRemove, modes }: ExprTreeProps<R>) {
  const setItem = (i: number, item: HomebrewExprNode<R>) => onChange({ ...node, items: node.items.map((x, j) => (j === i ? item : x)) });
  const removeItem = (i: number) => onChange({ ...node, items: node.items.filter((_, j) => j !== i) });
  const modeChoices = GROUP_MODES.filter((m) => !modes || modes.includes(m.value));
  const own = problems.filter((p) => p.path === path);
  return (
    <div className={`hb-group ${depth > 0 ? "hb-group-nested" : ""}`} role="group" aria-label={depth === 0 ? label : `${label}: group`}>
      <div className="hb-group-head">
        {modeChoices.length > 1
          ? <ComboBox aria-label={`${label}: how the parts combine`} value={node.mode} onChange={(v) => onChange({ ...node, mode: v as HomebrewGroupMode })} options={modeChoices} />
          : <span className="hb-group-mode">{modeChoices[0]?.label}</span>}
        {onRemove && <button type="button" className="hb-chip-x" aria-label="Remove this group" onClick={onRemove}><Icon name="x" /></button>}
      </div>
      <RowProblems problems={own} />
      <ul className="hb-group-items">
        {node.items.map((item, i) => {
          const itemPath = join(path, `items.${i}`);
          if (item.node === "group") {
            return (
              <li key={i}>
                <ExprTree label={label} node={item} onChange={(g) => setItem(i, g)} editor={editor} problems={problems} path={itemPath} depth={depth + 1} onRemove={() => removeItem(i)} />
              </li>
            );
          }
          const kept = editor.kept(item.row);
          const rowProblems = problemsAt(problems, itemPath);
          return (
            <li key={i} className={`hb-row ${rowProblems.length ? "hb-row-bad" : ""}`} data-path={itemPath}>
              <div className="hb-row-main">
                {kept !== null
                  ? <KeptChip text={kept} onRemove={() => removeItem(i)} />
                  : <>
                    <span className="hb-row-controls">{editor.render(item.row, (row) => setItem(i, { node: "row", row }), `${label}, part ${i + 1}`)}</span>
                    <button type="button" className="hb-chip-x" aria-label={`Remove part ${i + 1}`} onClick={() => removeItem(i)}><Icon name="x" /></button>
                  </>}
              </div>
              <RowProblems problems={rowProblems} />
            </li>
          );
        })}
      </ul>
      <div className="hb-group-add">
        <button type="button" className="hb-pick-btn" onClick={() => onChange({ ...node, items: [...node.items, { node: "row", row: editor.make() }] })}><Icon name="plus" />{editor.addLabel}</button>
        {depth < MAX_DEPTH - 1 && (
          <button type="button" className="hb-pick-btn" onClick={() => onChange({ ...node, items: [...node.items, { node: "group", mode: "any", items: [{ node: "row", row: editor.make() }] }] })}>
            <Icon name="plus" />Add group
          </button>
        )}
      </div>
    </div>
  );
}
