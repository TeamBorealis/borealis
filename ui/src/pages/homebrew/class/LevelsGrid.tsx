import { useRef, useState } from "react";
import { useCreatorEnv } from "../env";
import type { HomebrewForm, HomebrewTableColumn } from "../../../api/contract";
import { Button, Dialog, Icon, NumericUpDown, TextBox } from "../../../components";
import { DescCheck } from "../fields/DescCheck";
import { usePhone } from "../../../lib/phone";
import { noteStatus } from "../statusNote";
import { NumbersControl } from "../blocks/valueControls";
import { ChildDialog, type ChildEditing, childKind, removeChecked } from "./ChildDialog";
import "./class.css";
import { usePinnedHead } from "./pinnedHead";
import { isSpellcastingColumn } from "../caster/slots";
import {
  LEVELS, cellValue, childOf, columnsOf, counterColumns, entriesOf, entryName, fullColumn, generatedOf, nameOf, proficiencyBonus,
  withChild, withChildLevel, withField, withoutChild, withoutEntry,
} from "./children";

/** The class form's LEVELS field and its children fields (a feature removed from the grid leaves the class). */
const LEVELS_FIELD = "levels";
const CHILD_FIELDS = [LEVELS_FIELD, "equipment"];

/** The levels a subclass's class gives subclass features at (the backend derives them from the class, WB-18). */
const SUBCLASS_LEVELS_FIELD = "subclassLevels";

function numbers(v: unknown): number[] {
  return Array.isArray(v) ? v.filter((n): n is number => typeof n === "number") : [];
}

/**
 * The class LEVELS sub-tab (spec 4.5, 13.8; Aurora's level-up panels + DataGrid): the features the class makes for itself
 * (ability score improvements, the subclass choice, the 2024 epic boon), then the 20-row grid LEVEL / PROF. BONUS /
 * FEATURES / the class table's counters. A feature chip opens the feature editor; the Plus of a row adds a feature at
 * that level (made by the backend, `homebrew_new_draft {parentId}`).
 *
 * A subclass's grid (spec 4.7, `featureType` "Archetype Feature") has no class parts: its rows at the levels its class gives
 * subclass features at are marked ("Subclass feature"), from the backend's last answer.
 */
export function LevelsGrid({ editing, featureType = "Class Feature" }: { editing: ChildEditing; featureType?: string }) {
  const { form } = editing;
  const env = useCreatorEnv();
  const [open, setOpen] = useState<string | null>(null);
  const [newColumn, setNewColumn] = useState("");
  const phone = usePhone();
  /** Phones (spec 16): the level whose sheet is open. */
  const [sheetLevel, setSheetLevel] = useState<number | null>(null);
  const isClass = featureType === "Class Feature";
  const entries = entriesOf(form, LEVELS_FIELD);
  const columns = isClass ? columnsOf(form) : [];
  // the spellcasting columns (a 2024 class's cantrips, prepared spells and slots) are the SPELLCASTING tab's
  const counters = counterColumns(columns).filter(({ column }) => !isSpellcastingColumn(column.key));
  const gridFrame = useRef<HTMLDivElement>(null);
  // HC10 Q3: the header row stays in view while the grid scrolls past (phones list the levels as rows instead)
  usePinnedHead(gridFrame, [phone, counters.length]);
  const feature = childKind(editing, featureType);
  const marked = new Set(isClass ? [] : numbers(env.answer?.fields[SUBCLASS_LEVELS_FIELD] ?? form.fields[SUBCLASS_LEVELS_FIELD]));

  const addAt = async (level: number) => {
    if (!feature) return;
    const key = await editing.newChild(feature.id, (f, k) => withChildLevel(f, LEVELS_FIELD, k, level));
    if (key) { setOpen(key); noteStatus(`Feature added at level ${level}.`); }
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
  // a saved feature is deleted on save: characters that use it are named first (spec 10.3)
  const remove = (key: string) => {
    const name = entryName(form, { child: key });
    void removeChecked(editing, key, `“${name}”`, () => {
      editing.edit((f) => withoutChild(f, key, CHILD_FIELDS));
      setOpen(null);
      noteStatus("Feature removed. Undo brings it back.");
    });
  };

  /** The FEATURES cell of a level: the features at it (a chip opens one), and the Plus that adds one. */
  const atLevel = (level: number) => entries.map((e, i) => ({ e, i })).filter(({ e }) => (e.level ?? 1) === level);
  const featuresAt = (level: number, labelled = false) => (
    <>
      {marked.has(level) && atLevel(level).length === 0 && <span className="hb-level-hint">Subclass feature</span>}
      {atLevel(level).map(({ e, i }) => (
        <span key={e.child ?? `${i}-${e.element?.id ?? ""}`} className={`hb-chip-item hb-feature-chip ${e.element?.missing ? "hb-missing" : ""}`}>
          {e.child
            ? <button type="button" className="hb-feature-name" data-feature={entryName(form, e)} onClick={() => setOpen(e.child!)}>{entryName(form, e)}</button>
            : <span className="hb-feature-name" title="Given from another pack: copy the class with its features to edit it.">{entryName(form, e)}</span>}
          {!e.child && <button type="button" className="hb-chip-x" aria-label={`Remove ${entryName(form, e)}`} onClick={() => editing.edit((f) => withoutEntry(f, LEVELS_FIELD, i))}><Icon name="x" /></button>}
        </span>
      ))}
      {feature && <button type="button" className="hb-chip-x hb-add-feature" aria-label={`Add a feature at level ${level}`} title={`Add a feature at level ${level}`}
        onClick={() => void addAt(level)}><Icon name="plus" />{labelled && "Add feature"}</button>}
    </>
  );
  const counterAt = (column: HomebrewTableColumn, index: number, level: number) => (
    <input aria-label={`${column.label || column.key} at level ${level}`} value={String(column.values[level - 1] ?? 0)}
      onChange={(ev) => setCell(index, level, ev.target.value)} />
  );
  /** A phone row's detail line: the proficiency bonus, then the features at the level (or the subclass mark). */
  const levelDetail = (level: number): string => {
    const names = atLevel(level).map(({ e }) => entryName(form, e));
    const what = names.length > 0 ? names.join(", ") : marked.has(level) ? "Subclass feature" : "No features";
    return `+${proficiencyBonus(level)} · ${what}`;
  };

  return (
    <div className="hb-levels-tab">
      {isClass && <GeneratedFeatures editing={editing} />}
      <div className="hb-section">
        <h3 className="hb-section-title">Levels</h3>
        {phone ? (
          // Phones (spec 16): one row per level; a tap opens that level's features (and counters) in a sheet.
          <ul className="hb-level-rows" aria-label="Features by level">
            {LEVELS.map((level) => (
              <li key={level} data-level={level} className={marked.has(level) ? "hb-marked-level" : undefined}>
                {/* the name and the detail line read as two phrases ("Level 4, +2 · Ability Score Improvement"): without
                    the label, TalkBack and VoiceOver ran them together as "Level 4+2" (WB-26 device check) */}
                <button type="button" className="hb-level-row" aria-label={`Level ${level}, ${levelDetail(level)}`} onClick={() => setSheetLevel(level)}>
                  <span className="hb-level-row-name">Level {level}</span>
                  <span className="hb-level-row-detail">{levelDetail(level)}</span>
                </button>
              </li>
            ))}
          </ul>
        ) : (
        <div className="hb-levels-wrap" ref={gridFrame}>
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
                <tr key={level} data-level={level} className={marked.has(level) ? "hb-marked-level" : undefined}>
                  <th scope="row">{level}</th>
                  <td>+{proficiencyBonus(level)}</td>
                  <td className="hb-level-features">{featuresAt(level)}</td>
                  {counters.map(({ column, index }) => <td key={index} className="hb-counter">{counterAt(column, index, level)}</td>)}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        )}
        {isClass && <div className="hb-add-column">
          <label htmlFor="hb-new-column">New column</label>
          <TextBox id="hb-new-column" value={newColumn} placeholder="Rage uses" onChange={(e) => setNewColumn(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") addColumn(); }} />
          <button type="button" className="mg-tool wide" onClick={addColumn} disabled={!newColumn.trim()}><Icon name="plus" />Add column</button>
          <p className="hb-field-help">Counters per level (uses, dice); a rule&apos;s maximum can read a column (&quot;class table&quot;).</p>
        </div>}
      </div>
      {phone && sheetLevel !== null && open === null && (
        <Dialog title={`Level ${sheetLevel}`} onClose={() => setSheetLevel(null)} className="hb-level-sheet"
          footer={<Button onClick={() => setSheetLevel(null)}>Close</Button>}>
          <p className="hb-field-help">Proficiency bonus +{proficiencyBonus(sheetLevel)}.</p>
          <div className="hb-level-features" role="group" aria-label={`Features at level ${sheetLevel}`}>{featuresAt(sheetLevel, true)}</div>
          {counters.map(({ column, index }) => (
            <label key={index} className="mg-f hb-field hb-counter">
              <span>{column.label || column.key}</span>
              {counterAt(column, index, sheetLevel)}
            </label>
          ))}
        </Dialog>
      )}
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
    if (levels.length === 0) void removeChecked(editing, gen.asi.key!, `“${nameOf(gen.asi)}”`, () => editing.edit((f) => withoutChild(f, gen.asi!.key!, CHILD_FIELDS)));
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
        {(newer || gen.epicBoon) && (
          <div className="mg-f hb-field hb-check-field" data-field="epicBoon">
            <DescCheck id="hb-epic-boon" label="Epic boon at level 19" checked={!!gen.epicBoon} onChange={(on) => {
              if (on) make("Epic Boon", { epicBoon: true }, 19);
              else if (gen.epicBoon) void removeChecked(editing, gen.epicBoon.key!, `“${nameOf(gen.epicBoon)}”`, () => editing.edit((f) => withoutChild(f, gen.epicBoon!.key!, CHILD_FIELDS)));
            }} />
            {/* a 2014 class has no epic boon: one read from content still gives its rules (P2a) */}
            {!newer && <p className="hb-field-help hb-edition-note" role="note">This part belongs to the 2024 edition. It is shown because it still gives its rules: clear it to remove them.</p>}
          </div>
        )}
      </div>
    </div>
  );
}
