import { type ReactNode, useId } from "react";
import { CheckBox } from "../../components";
import type { CharacterOptions, OptionDef } from "../../api/contract";
import { useCharacter } from "../../state/CharacterContext";
import { XpField } from "./XpField";
import "./manage.css";
import "./manage-fx15.css";

/** Section of the Edition flyout: 13 px SemiBold uppercase title, 11 px description (classic builder flyout section metrics). */
export function EdSection({ title, sub, children }: { title: string; sub?: ReactNode; children: ReactNode }) {
  return (
    <section className="v4-ed-sec" aria-label={title}>
      <h2>{title}</h2>
      {sub && <p className="v4-ed-sub">{sub}</p>}
      {children}
    </section>
  );
}

/** Classic builder DescriptionCheckBox: CheckBox, 11 px SemiBold header, description indented 24 px in #7F7F7F. `caps` uppercases the header. */
export function EdCheck({ title, description, checked, onChange, caps }: { title: string; description?: ReactNode; checked: boolean; onChange: (on: boolean) => void; caps?: boolean }) {
  const id = useId();
  return (
    <div className={`v4-ed-check${caps ? " caps" : ""}`}>
      <CheckBox aria-labelledby={`${id}-t`} aria-describedby={description ? `${id}-d` : undefined} checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="head" id={`${id}-t`}>{title}</span>
      {description && <span className="desc" id={`${id}-d`}>{description}</span>}
    </div>
  );
}

/** Field header (10 px SemiBold uppercase) over a control. */
export function EdField({ label, children }: { label: string; children: (id: string) => ReactNode }) {
  const id = useId();
  return <div className="v4-ed-field"><label htmlFor={id}>{label}</label>{children(id)}</div>;
}

/** The value of one option as stored on the character (built-ins and custom content options). */
export function optionValue(o: CharacterOptions, id: string): boolean {
  if (id === "multiclassing") return o.multiclassing;
  if (id === "feats") return o.feats;
  if (id === "averageHp") return o.averageHp;
  return o.custom?.[id] ?? false;
}

/** A change set for `setCharacterOptions` from a draft of option toggles. */
export function optionsPatch(o: CharacterOptions, draft: Record<string, boolean>): Partial<CharacterOptions> {
  const patch: Partial<CharacterOptions> = {};
  const custom: Record<string, boolean> = { ...o.custom };
  let customChanged = false;
  for (const [id, v] of Object.entries(draft)) {
    if (optionValue(o, id) === v) continue;
    if (id === "multiclassing" || id === "feats" || id === "averageHp") patch[id] = v;
    else { custom[id] = v; customChanged = true; }
  }
  if (customChanged) patch.custom = custom;
  return patch;
}

/** The three built-in options: the backend lists only content options (`availableOptions`, `listCharacterOptions`). */
export const BUILT_IN_OPTION_DEFS: OptionDef[] = [
  { id: "averageHp", name: "Average hit points", description: "Hit points per level use the average roll of the hit die.", locked: false, defaultOn: true },
  { id: "feats", name: "Feats", description: "Lets a character pick a feat in place of an Ability Score Improvement.", locked: false, defaultOn: true },
  { id: "multiclassing", name: "Multiclassing", description: "Permit taking levels in more than one class.", locked: false, defaultOn: true },
];
export const isBuiltInOption = (id: string): boolean => BUILT_IN_OPTION_DEFS.some((d) => d.id === id);

/** The built-ins, then the content options by name (Aurora lists both on its New Character and Manage Character Options sliders). */
export const withBuiltIns = (content: OptionDef[]): OptionDef[] =>
  [...BUILT_IN_OPTION_DEFS, ...content.filter((d) => !isBuiltInOption(d.id))];

/** One card order for both option surfaces (new character and F8): AVERAGE HIT POINTS, FEATS, MULTICLASSING, then content options. */
const OPTION_ORDER = ["averageHp", "feats", "multiclassing"];
export function orderOptions(defs: OptionDef[]): OptionDef[] {
  const rank = (id: string) => { const i = OPTION_ORDER.indexOf(id); return i < 0 ? OPTION_ORDER.length : i; };
  return [...defs].sort((a, b) => rank(a.id) - rank(b.id));
}

/** The classic builder's dashed option cards: SemiBold uppercase title, 11 px hint, "custom" / "locked" tags. */
export function OptionCards({ defs, value, onToggle }: { defs: OptionDef[]; value: (id: string) => boolean; onToggle: (id: string, on: boolean) => void }) {
  return (
    <div className="v4-opts mg-opt-cards">
      {orderOptions(defs).map((d) => {
        const builtin = isBuiltInOption(d.id);
        return (
          <div className={`v4-opt${d.locked ? " locked" : ""}`} key={d.id}>
            <label>
              <CheckBox checked={value(d.id)} disabled={d.locked} onChange={(e) => onToggle(d.id, e.target.checked)} />
              {d.name}
              {!builtin && <span className="mg-tag">custom</span>}
              {d.locked && <span className="mg-tag">locked</span>}
            </label>
            <p>{d.description}</p>
          </div>
        );
      })}
    </div>
  );
}

/** House rules (Borealis-only): unrestricted point buy and the attunement limit. */
export function HouseRules() {
  const { snapshot, mutate } = useCharacter();
  if (!snapshot) return null;
  const o = snapshot.options;
  const set = (patch: Partial<typeof o>) => void mutate((a, id) => a.setCharacterOptions({ characterId: id, options: patch }));
  return (
    <EdSection title="House rules" sub="Table-specific changes to the standard rules.">
      <EdCheck title="Unrestricted point buy" description="Allow spending beyond the budget (custom tables)." checked={o.pointBuyUnrestricted} onChange={(on) => set({ pointBuyUnrestricted: on })} />
      <EdField label="Attunement limit">
        {() => <XpField label="Attunement limit" noun="attunement limit" min={1} max={10} step={1} value={o.attunementLimit} onChange={(n) => set({ attunementLimit: n })} />}
      </EdField>
    </EdSection>
  );
}
