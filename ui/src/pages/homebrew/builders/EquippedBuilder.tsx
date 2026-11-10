import type { HomebrewBuilderProblem, HomebrewEquipped, HomebrewEquippedRow, HomebrewExprNode, HomebrewExprValue, HomebrewSlot, HomebrewSlotTest } from "../../../api/contract";
import { ComboBox } from "../../../components";
import { IdRefField } from "../pickers/RefField";
import { ExprTree, KeptChip, type RowEditor } from "./ExprTree";
import { emptyGroup, SLOT_TESTS, SLOT_WORDS, TEST_WORDS, withModel } from "./words";

type Row = HomebrewEquippedRow;
type RowKind = Exclude<Row["row"], "kept">;
type Group = Extract<HomebrewExprNode<Row>, { node: "group" }>;

const ROW_WORDS: readonly { value: RowKind; label: string }[] = [
  { value: "slot", label: "Wearing or holding in" },
  { value: "attuned", label: "Attuned to" },
  { value: "wearing", label: "Has equipped" },
];

/** Element types a slot's "specific item" offers. */
const SLOT_TYPES: Readonly<Record<HomebrewSlot, string[]>> = {
  armor: ["Armor", "Magic Item"], shield: ["Armor", "Magic Item"], mainHand: ["Weapon", "Magic Item"], offHand: ["Weapon", "Magic Item"],
};
const ITEM_TYPES = ["Item", "Weapon", "Armor", "Magic Item"];

function newRow(kind: RowKind): Row {
  if (kind === "slot") return { row: kind, slot: "armor", test: { kind: "none" } };
  return { row: kind, item: "" };
}

function newTest(kind: HomebrewSlotTest["kind"]): HomebrewSlotTest {
  return kind === "item" ? { kind, item: "" } : { kind } as HomebrewSlotTest;
}

/** Items are stored lower case (the engine compares them so); the picker hands back the id, written lower case here. */
const itemIds = (item: string): string[] => (item ? [item] : []);

function EquippedRowControls({ row, onRow, label }: { row: Row; onRow: (r: Row) => void; label: string }) {
  if (row.row === "kept") return null;
  const kind = <ComboBox aria-label={`${label}: test`} value={row.row} options={ROW_WORDS} onChange={(v) => onRow(newRow(v as RowKind))} />;
  if (row.row === "slot") {
    const tests = SLOT_TESTS[row.slot].map((k) => ({ value: k, label: TEST_WORDS[k] }));
    return (
      <>
        {kind}
        <ComboBox aria-label={`${label}: slot`} value={row.slot} options={(Object.keys(SLOT_WORDS) as HomebrewSlot[]).map((s) => ({ value: s, label: SLOT_WORDS[s] }))}
          onChange={(v) => { const slot = v as HomebrewSlot; onRow({ ...row, slot, test: SLOT_TESTS[slot].includes(row.test.kind) ? row.test : { kind: "any" } }); }} />
        <ComboBox aria-label={`${label}: what`} value={row.test.kind} options={tests} onChange={(v) => onRow({ ...row, test: newTest(v as HomebrewSlotTest["kind"]) })} />
        {row.test.kind === "item" && (
          <IdRefField label={`${label}: item`} target={{ types: SLOT_TYPES[row.slot] }} ids={itemIds(row.test.item)} multiple={false}
            onIds={(ids) => onRow({ ...row, test: { kind: "item", item: (ids[0] ?? "").toLowerCase() } })} />
        )}
      </>
    );
  }
  return (
    <>
      {kind}
      <IdRefField label={`${label}: item`} target={{ types: row.row === "attuned" ? ["Magic Item"] : ITEM_TYPES }} ids={itemIds(row.item)} multiple={false}
        onIds={(ids) => onRow({ ...row, item: (ids[0] ?? "").toLowerCase() })} />
    </>
  );
}

const EDITOR: RowEditor<Row> = {
  render: (row, onRow, label) => <EquippedRowControls row={row} onRow={onRow} label={label} />,
  make: () => newRow("slot"),
  kept: (row) => (row.row === "kept" ? row.text : null),
  addLabel: "Add test",
};

export interface EquippedBuilderProps {
  label: string;
  value: HomebrewExprValue<HomebrewEquipped> | null;
  onChange: (value: HomebrewExprValue<HomebrewEquipped>) => void;
  problems: readonly HomebrewBuilderProblem[];
}

/** The "while wearing/holding" builder (spec 6.3, `equipped`): armor, shield, main hand, off hand, attuned, a specific item. */
export function EquippedBuilder({ label, value, onChange, problems }: EquippedBuilderProps) {
  if (value?.value.shape === "kept") {
    return <KeptChip text={value.value.text} reason={value.value.reason} what="test" onRemove={() => onChange(withModel(value, emptyGroup<Row>()))} />;
  }
  const root = (value?.value.shape === "model" && value.value.model.node === "group" ? value.value.model : emptyGroup<Row>()) as Group;
  return <ExprTree label={label} node={root} onChange={(g) => onChange(withModel(value, g))} editor={EDITOR} problems={problems} path="" />;
}
