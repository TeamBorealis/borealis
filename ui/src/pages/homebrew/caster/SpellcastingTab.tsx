import type { HomebrewForm, Ruleset } from "../../../api/contract";
import { Icon } from "../../../components";
import { noteStatus } from "../statusNote";
import { type ChildEditing, childKind } from "../class/ChildDialog";
import { childProblems, withChild, withChildLevel } from "../class/children";
import { FieldView } from "../FieldView";
import { CasterSection } from "./CasterSection";

/** The class's LEVELS field (its Spellcasting feature is on level 1 of it). */
const LEVELS_FIELD = "levels";
/** The names the SRD gives a class's spellcasting feature. */
const CASTER_NAMES = ["spellcasting", "pact magic"];

/** The class's Spellcasting feature: the child that gives spellcasting, else one named like it (turned off). */
export function spellcastingChild(form: HomebrewForm): number {
  const on = form.children.findIndex((c) => c.fields.casts === true);
  if (on >= 0) return on;
  return form.children.findIndex((c) => typeof c.fields.name === "string" && CASTER_NAMES.includes(c.fields.name.trim().toLowerCase()));
}

/** What a new section starts with: a full caster that prepares from its whole list (the class's name as its list); a
 * subclass's, a third caster that knows its spells (the Eldritch Knight shape). */
const NEW_SECTION: Record<string, unknown> = { casts: true, casterType: "full", preparation: "prepared", multiclassSlots: "auto" };
const NEW_SUBCLASS_SECTION: Record<string, unknown> = { casts: true, casterType: "third", preparation: "known", multiclassSlots: "auto" };

/** Where a form's Spellcasting feature goes: a class's at level 1, a subclass's at the level it is chosen at. */
function featurePlace(form: HomebrewForm): { type: string; level: number; noun: string; section: Record<string, unknown> } {
  if (form.kind !== "subclass") return { type: "Class Feature", level: 1, noun: "class", section: NEW_SECTION };
  const gained = form.fields.levelGained;
  return { type: "Archetype Feature", level: typeof gained === "number" ? gained : 3, noun: "subclass", section: NEW_SUBCLASS_SECTION };
}

/**
 * The class form's SPELLCASTING sub-tab (spec 4.5, 4.6): the spellcasting section of the class's Spellcasting feature
 * (a child at level 1, written the way the SRD stores it), and that feature's other rules (always-prepared spells,
 * ritual casting, a spellbook: rule blocks). A class that doesn't cast offers "Add spellcasting". A subclass's (WB-18)
 * is the same with a subclass feature at its subclass level (a third caster).
 */
export function SpellcastingTab({ editing, expert }: { editing: ChildEditing; expert: boolean }) {
  const { form } = editing;
  const index = spellcastingChild(form);
  const child = index >= 0 ? form.children[index] : undefined;
  const place = featurePlace(form);
  const feature = childKind(editing, place.type);
  const fields = (feature?.fields ?? []).filter((f) => f.tab === "spellcasting");
  const setChild = (c: HomebrewForm, patch: Record<string, unknown>, group: string | null) =>
    editing.edit((f) => {
      const cur = f.children.find((x) => x.key === c.key) ?? c;
      return withChild(f, c.key!, { ...cur, fields: { ...cur.fields, ...patch } });
    }, group);

  if (!child || child.fields.casts !== true) {
    const add = async () => {
      if (child) { setChild(child, place.section, null); return; }
      if (!feature) return;
      const key = await editing.newChild(feature.id, (f, k) => {
        const c = f.children.find((x) => x.key === k);
        const placed = withChildLevel(f, LEVELS_FIELD, k, place.level);
        return c ? withChild(placed, k, { ...c, fields: { ...c.fields, name: "Spellcasting", ...place.section } }) : placed;
      });
      if (key) noteStatus(`Spellcasting added at level ${place.level}.`);
    };
    return (
      <div className="hb-section hb-caster-empty">
        <p className="hb-pending">This {place.noun} doesn&apos;t cast spells.</p>
        <button type="button" className="mg-tool wide" onClick={() => void add()} disabled={!feature && !child}><Icon name="plus" />Add spellcasting</button>
        <p className="hb-field-help">A Spellcasting feature at level {place.level} gets the caster type, ability, spell list and spells by level{place.noun === "subclass" ? ", counted under the subclass's name and gated on the class level" : ""}.</p>
      </div>
    );
  }
  const problems = childProblems(editing.problems, index);
  const rules = feature?.fields.find((f) => f.control === "blocks");
  return (
    <div className="hb-spellcasting-tab" data-child={child.key}>
      <CasterSection fields={fields} form={child} problems={problems} edition={editing.edition as Ruleset | null} expert={expert} idPrefix={`c-${child.key}-`}
        onFields={(patch, group) => setChild(child, patch, group)} />
      {rules && (
        <div className="hb-section">
          <h3 className="hb-section-title">Spellcasting rules</h3>
          <p className="hb-field-help">Spells always prepared (Gives a spell), ritual casting, a spellbook and a spellcasting focus are rules of the {String(child.fields.name || "Spellcasting")} feature.</p>
          <div className="mg-grid two">
            <FieldView field={rules} value={child.blocks} problems={problems} kindId={child.kind} idPrefix={`c-${child.key}-`}
              onChange={(b) => editing.edit((f) => withChild(f, child.key!, { ...(f.children.find((x) => x.key === child.key) ?? child), blocks: b as HomebrewForm["blocks"] }))} />
          </div>
        </div>
      )}
    </div>
  );
}
