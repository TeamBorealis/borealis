import { useState } from "react";
import type { HomebrewForm, HomebrewTableColumn } from "../../../api/contract";
import { CheckBox, Icon, NumericUpDown, TextBox } from "../../../components";
import { setStatus } from "../../../state/status";
import { NumbersControl } from "../blocks/valueControls";
import { ChildDialog, type ChildEditing, childKind } from "./ChildDialog";
import "./class.css";
import {
  LEVELS, cellValue, childOf, columnsOf, counterColumns, entriesOf, entryName, fullColumn, generatedOf, proficiencyBonus,
  withChild, withChildLevel, withField, withoutChild, withoutEntry,
} from "./children";

/** The class form's LEVELS field and its children fields (a feature removed from the grid leaves the class). */
const LEVELS_FIELD = "levels";
const CHILD_FIELDS = [LEVELS_FIELD, "equipment"];

/**
 * The class LEVELS sub-tab (spec 4.5, 13.8; Aurora's level-up panels + DataGrid): the features the class makes for itself
 * (ability score improvements, the subclass choice, the 2024 epic boon), then the 20-row grid LEVEL / PROF. BONUS /
 * FEATURES / the class table's counters. A feature chip opens the feature editor; the Plus of a row adds a feature at
 * that level (made by the backend, `homebrew_new_draft {parentId}`).
 */
export function LevelsGrid({ editing }: { editing: ChildEditing }) {
  const { form } = editing;
  const [open, setOpen] = useState<string | null>(null);
  const [newColumn, setNewColumn] = useState("");
  const entries = entriesOf(form, LEVELS_FIELD);
  const columns = columnsOf(form);
  const counters = counterColumns(columns);
  const feature = childKind(editing, "Class Feature");

  const addAt = async (level: number) => {
    if (!feature) return;
    const key = await editing.newChild(feature.id, (f, k) => withChildLevel(f, LEVELS_FIELD, k, level));
    if (key) { setOpen(key); setStatus(`Feature added at level ${level}.`); }
  };
  const setColumns = (next: HomebrewTableColumn[]) => editing.edit((f) => withField(f, "table", next));
  const setCell = (index: number, level: number, text: string) => {
    const next = columns.map((c, i) => (i === index ? { ...fullColumn(c), values: fullColumn(c).values.map((v, j) => (j === level - 1 ? cellValue(text) : v)) } : c));
    editing.edit((f) => withField(f, "table", next), `table:${index}:${level}`);
  };
  const addColumn = () => {
    const label = newColumn.trim();
    if (!label) return;
    setColumns([...columns, { key: "", label, values: LEVELS.map(() => 0) }]);
    setNewColumn("");
  };
  const remove = (key: string) => {
    editing.edit((f) => withoutChild(f, key, CHILD_FIELDS));
    setOpen(null);
    setStatus("Feature removed. Undo brings it back.");
  };

  return (
    <div className="hb-levels-tab">
      <GeneratedFeatures editing={editing} />
      <div className="hb-section">
        <h3 className="hb-section-title">Levels</h3>
        <div className="hb-levels-wrap">
          <table className="ap-grid hb-levels" aria-label="Features by level">
            <thead>
              <tr>
                <th scope="col">Level</th>
                <th scope="col">Prof. bonus</th>
                <th scope="col">Features</th>
                {counters.map(({ column, index }) => (
                  <th scope="col" key={index}>
                    <span>{column.label || column.key}</span>
                    <button type="button" className="hb-chip-x" aria-label={`Remove the column ${column.label || column.key}`}
                      onClick={() => setColumns(columns.filter((_, i) => i !== index))}><Icon name="x" /></button>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {LEVELS.map((level) => (
                <tr key={level} data-level={level}>
                  <th scope="row">{level}</th>
                  <td>+{proficiencyBonus(level)}</td>
                  <td className="hb-level-features">
                    {entries.map((e, i) => (e.level ?? 1) === level && (
                      <span key={e.child ?? `${i}-${e.element?.id ?? ""}`} className={`hb-chip-item hb-feature-chip ${e.element?.missing ? "hb-missing" : ""}`}>
                        {e.child
                          ? <button type="button" className="hb-feature-name" data-feature={entryName(form, e)} onClick={() => setOpen(e.child!)}>{entryName(form, e)}</button>
                          : <span className="hb-feature-name" title="Given from another pack: copy the class with its features to edit it.">{entryName(form, e)}</span>}
                        {!e.child && <button type="button" className="hb-chip-x" aria-label={`Remove ${entryName(form, e)}`} onClick={() => editing.edit((f) => withoutEntry(f, LEVELS_FIELD, i))}><Icon name="x" /></button>}
                      </span>
                    ))}
                    {feature && <button type="button" className="hb-chip-x hb-add-feature" aria-label={`Add a feature at level ${level}`} title={`Add a feature at level ${level}`}
                      onClick={() => void addAt(level)}><Icon name="plus" /></button>}
                  </td>
                  {counters.map(({ column, index }) => (
                    <td key={index} className="hb-counter">
                      <input aria-label={`${column.label || column.key} at level ${level}`} value={String(column.values[level - 1] ?? 0)}
                        onChange={(ev) => setCell(index, level, ev.target.value)} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="hb-add-column">
          <label htmlFor="hb-new-column">New column</label>
          <TextBox id="hb-new-column" value={newColumn} placeholder="Rage uses" onChange={(e) => setNewColumn(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") addColumn(); }} />
          <button type="button" className="mg-tool wide" onClick={addColumn} disabled={!newColumn.trim()}><Icon name="plus" />Add column</button>
          <p className="hb-field-help">Counters per level (uses, dice); a rule&apos;s maximum can read a column (&quot;class table&quot;).</p>
        </div>
      </div>
      {open && <ChildDialog editing={editing} childKey={open} levelField={LEVELS_FIELD} onRemove={() => remove(open)} onClose={() => setOpen(null)} />}
    </div>
  );
}

/**
 * The features every class makes for itself (spec 4.5): ability score improvements (one choice per level), the subclass
 * choice (its label and level), and the 2024 epic boon at 19. Their rules are written by the backend from the class's
 * name; here they are a list of levels, a label and a check box.
 */
function GeneratedFeatures({ editing }: { editing: ChildEditing }) {
  const { form } = editing;
  const gen = generatedOf(form);
  const feature = childKind(editing, "Class Feature");
  const newer = editing.edition !== "dnd2014";
  const make = (name: string, fields: Record<string, unknown>, level: number) => {
    if (!feature) return;
    void editing.newChild(feature.id, (f, key) => {
      const c = f.children.find((x) => x.key === key);
      const placed = withChildLevel(f, LEVELS_FIELD, key, level);
      return c ? withChild(placed, key, { ...c, fields: { ...c.fields, name, ...fields } }) : placed;
    });
  };
  const setChild = (child: HomebrewForm, fields: Record<string, unknown>, level?: number) => editing.edit((f) => {
    const next = withChild(f, child.key!, { ...child, fields: { ...child.fields, ...fields } });
    return level === undefined ? next : withChildLevel(next, LEVELS_FIELD, child.key!, level);
  });
  const levelOf = (c: HomebrewForm | undefined) => entriesOf(form, LEVELS_FIELD).find((e) => childOf(form, e) === c)?.level ?? 1;
  const asiLevels = (gen.asi?.fields.asiLevels as number[] | undefined) ?? [];
  const setAsi = (levels: number[]) => {
    if (!gen.asi) { if (levels.length) make("Ability Score Improvement", { asiLevels: levels }, levels[0]!); return; }
    if (levels.length === 0) editing.edit((f) => withoutChild(f, gen.asi!.key!, CHILD_FIELDS));
    else setChild(gen.asi, { asiLevels: levels }, levels[0]);
  };
  const subLabel = typeof gen.subclass?.fields.subclassLabel === "string" ? gen.subclass.fields.subclassLabel : "";
  return (
    <div className="hb-section hb-generated">
      <h3 className="hb-section-title">Made by the class</h3>
      <div className="mg-grid two">
        <div className="mg-f hb-field hb-wide" data-field="asiLevels">
          <label>Ability score improvements at levels</label>
          <NumbersControl label="Ability score improvements at levels" value={asiLevels} min={1} max={20} onChange={(v) => setAsi((v ?? []) as number[])} />
          <p className="hb-field-help">{newer ? "A choice of a General feat (the Ability Score Improvement feat suggested) at each level." : "An ability score improvement or a feat at each level."}</p>
        </div>
        <div className="mg-f hb-field" data-field="subclassLevel">
          <label htmlFor="hb-subclass-level">Subclass choice at level</label>
          {gen.subclass
            ? <span id="hb-subclass-level" className="hb-number"><NumericUpDown label="Subclass choice at level" value={levelOf(gen.subclass)} min={1} max={20}
              onChange={(n) => editing.edit((f) => withChildLevel(f, LEVELS_FIELD, gen.subclass!.key!, n))} /></span>
            : <button id="hb-subclass-level" type="button" className="mg-tool wide" onClick={() => make("Subclass", { subclassLabel: "Subclass" }, 3)}><Icon name="plus" />Add a subclass choice</button>}
        </div>
        {gen.subclass && (
          <div className="mg-f hb-field" data-field="subclassLabel">
            <label htmlFor="hb-subclass-label">Subclass choice name</label>
            <TextBox id="hb-subclass-label" value={subLabel} onChange={(e) => editing.edit((f) => withChild(f, gen.subclass!.key!, { ...gen.subclass!, fields: { ...gen.subclass!.fields, subclassLabel: e.target.value } }), "subclassLabel")} />
            <p className="hb-field-help">Sacred Oath, Martial Archetype: subclasses of this class are offered there.</p>
          </div>
        )}
        {newer && (
          <div className="mg-f hb-field" data-field="epicBoon">
            <label htmlFor="hb-epic-boon">Epic boon at level 19</label>
            <span className="hb-check"><CheckBox id="hb-epic-boon" checked={!!gen.epicBoon} onChange={(e) => {
              if (e.target.checked) make("Epic Boon", { epicBoon: true }, 19);
              else if (gen.epicBoon) editing.edit((f) => withoutChild(f, gen.epicBoon!.key!, CHILD_FIELDS));
            }} /></span>
          </div>
        )}
      </div>
    </div>
  );
}
