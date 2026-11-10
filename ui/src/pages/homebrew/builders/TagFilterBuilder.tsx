import type { HomebrewBuilderProblem, HomebrewExprNode, HomebrewExprValue, HomebrewTagFilter, HomebrewTagRow } from "../../../api/contract";
import { ComboBox, EditableComboBox, Icon } from "../../../components";
import { useApi } from "../../../state/ApiContext";
import { useAsync } from "../../../state/useAsync";
import { namesInScope, useCreatorEnv } from "../env";
import { IdRefField } from "../pickers/RefField";
import { ExprTree, KeptChip, type RowEditor } from "./ExprTree";
import { emptyGroup, withModel } from "./words";

type Row = HomebrewTagRow;
type RowKind = Exclude<Row["row"], "kept">;
type Group = Extract<HomebrewExprNode<Row>, { node: "group" }>;

const ROW_WORDS: readonly { value: RowKind; label: string }[] = [
  { value: "tag", label: "Tagged" },
  { value: "element", label: "This element" },
  { value: "classList", label: "On the class's spell list" },
  { value: "castableLevel", label: "Of a level the class can cast" },
  { value: "choice", label: "The option chosen for" },
];

function newRow(kind: RowKind): Row {
  switch (kind) {
    case "tag": return { row: kind, tag: "" };
    case "element": return { row: kind, id: "" };
    case "choice": return { row: kind, name: "" };
    default: return { row: kind };
  }
}

/** One tag filter row's controls. */
function TagRowControls({ row, onRow, label, targetTypes }: { row: Row; onRow: (r: Row) => void; label: string; targetTypes: readonly string[] }) {
  const env = useCreatorEnv();
  if (row.row === "kept") return null;
  const kind = <ComboBox aria-label={`${label}: test`} value={row.row} options={ROW_WORDS} onChange={(v) => onRow(newRow(v as RowKind))} />;
  switch (row.row) {
    case "tag":
      return <>{kind}<EditableComboBox label={`${label}: tag`} aria-label={`${label}: tag`} value={row.tag} options={[]} onChange={(tag) => onRow({ ...row, tag })} /></>;
    case "element":
      return <>{kind}<IdRefField label={`${label}: element`} target={{ types: [...targetTypes] }} ids={row.id ? [row.id] : []} multiple={false} onIds={(ids) => onRow({ ...row, id: ids[0] ?? "" })} /></>;
    case "choice":
      return <>{kind}<EditableComboBox label={`${label}: named choice`} aria-label={`${label}: named choice`} value={row.name} options={namesInScope(env, "choice")} onChange={(name) => onRow({ ...row, name })} /></>;
    default:
      return kind;
  }
}

/** Tags of a flat Any of (or one-tag) filter: what the picker data can count without the engine. */
function countableTags(root: Group): string[] | null {
  const rows = root.items.map((i) => (i.node === "row" && i.row.row === "tag" ? i.row.tag.trim() : null));
  if (rows.length === 0 || rows.some((t) => !t)) return null;
  return root.mode === "any" || rows.length === 1 ? rows as string[] : null;
}

/** "12 match" under the filter (spec 6.3); zero is a warning. Counts flat tag filters only. */
function MatchCount({ root, targetTypes }: { root: Group; targetTypes: readonly string[] }) {
  const api = useApi();
  const env = useCreatorEnv();
  const tags = root.mode === "none" ? null : countableTags(root);
  const key = tags?.join("\u0000") ?? "";
  const count = useAsync(
    () => (tags ? api.homebrewReferenceOptions({ target: { types: [...targetTypes], tags }, query: "", filters: {}, page: { offset: 0, limit: 1 }, ...(env.draftId ? { draftId: env.draftId } : {}) }).then((p) => p.total) : Promise.resolve(null)),
    [api, key, targetTypes.join("|"), env.draftId],  
  );
  if (!tags || count.data === null || count.data === undefined) return null;
  return count.data === 0
    ? <p className="hb-count hb-count-zero" role="status"><Icon name="alert" />Nothing loaded matches this filter.</p>
    : <p className="hb-count" role="status">{count.data} match</p>;
}

export interface TagFilterBuilderProps {
  label: string;
  value: HomebrewExprValue<HomebrewTagFilter> | null;
  onChange: (value: HomebrewExprValue<HomebrewTagFilter>) => void;
  problems: readonly HomebrewBuilderProblem[];
  /** The element types the filter picks from (a select's "What to choose"), for element rows and the count. */
  targetTypes?: readonly string[];
}

const EMPTY: HomebrewTagFilter = { specific: false, root: emptyGroup<Row>() };

/**
 * The tag filter builder (spec 6.3, `supports`): *Specific elements* (a list of picked elements) or *Anything tagged*
 * (tag rows, the class's spell list, a castable level, a named choice's option; All of / Any of / None of), with a live
 * count of what loaded content matches.
 */
export function TagFilterBuilder({ label, value, onChange, problems, targetTypes = [] }: TagFilterBuilderProps) {
  if (value?.value.shape === "kept") {
    return <KeptChip text={value.value.text} reason={value.value.reason} what="filter" onRemove={() => onChange(withModel(value, EMPTY))} />;
  }
  const model = value?.value.shape === "model" ? value.value.model : EMPTY;
  const root = (model.root.node === "group" ? model.root : { node: "group", mode: "all", items: [model.root] }) as Group;
  const set = (next: HomebrewTagFilter) => onChange(withModel(value, next));
  const editor: RowEditor<Row> = {
    render: (row, onRow, l) => <TagRowControls row={row} onRow={onRow} label={l} targetTypes={targetTypes} />,
    make: () => newRow("tag"),
    kept: (row) => (row.row === "kept" ? row.text : null),
    addLabel: "Add test",
  };
  const elementIds = root.items.flatMap((i) => (i.node === "row" && i.row.row === "element" ? [i.row.id] : []));
  const keptRows = root.items.filter((i) => i.node === "row" && i.row.row === "kept");
  const setSpecific = (specific: boolean) => set({ specific, root: specific ? { node: "group", mode: "any", items: [] } : emptyGroup<Row>() });
  return (
    <div className="hb-tagfilter">
      <ComboBox aria-label={`${label}: from`} value={model.specific ? "specific" : "tagged"} onChange={(v) => setSpecific(v === "specific")}
        options={[{ value: "tagged", label: "Anything that passes these tests" }, { value: "specific", label: "Specific elements" }]} />
      {model.specific ? (
        <>
          <IdRefField label={`${label}: elements`} target={{ types: [...targetTypes], multiple: true }} ids={elementIds} multiple
            onIds={(ids) => set({ specific: true, root: { node: "group", mode: "any", items: [...keptRows, ...ids.map((id) => ({ node: "row" as const, row: { row: "element" as const, id } }))] } })} />
          {keptRows.map((k, i) => k.node === "row" && k.row.row === "kept" && (
            <KeptChip key={i} text={k.row.text} onRemove={() => set({ specific: true, root: { ...root, items: root.items.filter((x) => x !== k) } })} />
          ))}
        </>
      ) : (
        <>
          <ExprTree label={label} node={root} onChange={(g) => set({ specific: false, root: g })} editor={editor} problems={problems} path="root" />
          <MatchCount root={root} targetTypes={targetTypes} />
        </>
      )}
    </div>
  );
}
