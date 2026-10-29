import { CheckBox } from "../../components/controls";
import { useState } from "react";
import { ABILITY_KEYS, type Api, type AbilityMethod, type CharacterSnapshot, type OptionDef, type Ruleset } from "../../api/contract";
import { Button, ComboBox, Expander, Flyout, IconButton, TextBox } from "../../components";
import { methodLabel, randomName } from "../../lib/format";
import { useApi } from "../../state/ApiContext";
import { useSettings } from "../../state/SettingsContext";
import { useAsync } from "../../state/useAsync";
import { BUILT_IN_OPTION_DEFS } from "../manage/OptionsTab";
import "./characters.css";

export interface NewCharacterForm {
  name: string; ruleset: Ruleset; startingLevel: number; gender: string; abilityMethod: AbilityMethod;
  multiclassing: boolean; feats: boolean; averageHp: boolean;
  /** Content options the user switched (id -> on); an option left alone keeps its `defaultOn`. */
  custom: Record<string, boolean>;
  /** Standard-array values in STR..CHA order (the arrangement made in the flyout). */
  standardArray: number[];
}

/** Every listed content option's state for `create_character` (the user's switch, else the option's default). */
export const customOptions = (defs: OptionDef[], f: Pick<NewCharacterForm, "custom">): Record<string, boolean> =>
  Object.fromEntries(defs.map((d) => [d.id, f.custom[d.id] ?? d.defaultOn]));

const METHODS: AbilityMethod[] = ["point_buy", "standard_array", "roll_4d6", "roll_3d6", "manual"];
const ABILITIES = ["STR", "DEX", "CON", "INT", "WIS", "CHA"];
export const STANDARD_ARRAY = [15, 14, 13, 12, 10, 8];
const EDITIONS: { id: Ruleset; year: string; note: string }[] = [
  { id: "dnd2024", year: "2024", note: "Revised rules (SRD 5.2)" },
  { id: "dnd2014", year: "2014", note: "Classic rules (SRD 5.1)" },
];
const LEVELS = Array.from({ length: 20 }, (_, i) => ({ value: String(i + 1), label: String(i + 1) }));
const GENDERS = ["", "Female", "Male", "Non-binary", "Construct"].map((g) => ({ value: g, label: g || "Unspecified" }));

function abilityPreview(m: AbilityMethod, arranged: number[]): string[] {
  if (m === "standard_array") return arranged.map(String);
  if (m === "point_buy") return ABILITIES.map(() => "8");
  if (m === "manual") return ABILITIES.map(() => "10");
  return ABILITIES.map(() => "?");
}

/** Swap two tiles of the arrangement (new array; the input is untouched). */
export const swapTiles = (arr: number[], a: number, b: number): number[] =>
  arr.map((v, i) => (i === a ? arr[b]! : i === b ? arr[a]! : v));

/** After creation, apply a rearranged standard array through the existing assign command (each call swaps, so the final order is exact). Always applied, including the default order: a mock/backend may start standard-array characters unassigned. */
export async function applyStandardArray(api: Api, snap: CharacterSnapshot, f: NewCharacterForm): Promise<CharacterSnapshot> {
  if (f.abilityMethod !== "standard_array") return snap;
  let cur = snap;
  for (let i = 0; i < ABILITY_KEYS.length; i++) {
    cur = await api.assignStandardArray({ characterId: snap.id, ability: ABILITY_KEYS[i]!, value: f.standardArray[i]! });
  }
  return cur;
}

const OPTIONS = BUILT_IN_OPTION_DEFS.map((d) => ({ key: d.id as "averageHp" | "feats" | "multiclassing", title: d.name, hint: d.description }));

const methodOptions = METHODS.map((m) => ({ value: m, label: m === "standard_array" ? "Standard Array (15, 14, 13, 12, 10, 8)" : methodLabel(m) }));
const editionOptions = EDITIONS.map((e) => ({ value: e.id, label: `${e.year} - ${e.note}` }));

/** Classic builder new-character flyout: portraits, collapsible CHARACTER DETAILS (name with dice inside the field, level, gender, abilities option and a rearrangeable standard array) and CHARACTER OPTIONS. */
export function NewCharacterDialog({ onCreate, onClose }: { onCreate: (f: NewCharacterForm, contentOptions: OptionDef[]) => void; onClose: () => void }) {
  const { settings } = useSettings();
  const api = useApi();
  const [f, setF] = useState<NewCharacterForm>({
    name: "", ruleset: settings.defaultRuleset, startingLevel: 1, gender: "", abilityMethod: settings.abilityMethod,
    multiclassing: true, feats: true, averageHp: true, custom: {}, standardArray: STANDARD_ARRAY,
  });
  // Issue 4: the content's own options (Aurora lists them after the built-ins, tagged "custom")
  const contentOptions = useAsync(() => api.listCharacterOptions({ ruleset: f.ruleset }), [api, f.ruleset]).data ?? [];
  const create = () => onCreate(f, contentOptions);
  const [grab, setGrab] = useState<number | null>(null);
  const set = <K extends keyof NewCharacterForm>(k: K, v: NewCharacterForm[K]) => setF((cur) => ({ ...cur, [k]: v }));
  const swap = (a: number, b: number) => { if (a !== b) set("standardArray", swapTiles(f.standardArray, a, b)); };
  const tapTile = (i: number) => {
    if (f.abilityMethod !== "standard_array") return;
    if (grab === null) setGrab(i); else { swap(grab, i); setGrab(null); }
  };
  const initials = f.name.split(/\s+/).filter(Boolean).slice(0, 2).map((w) => w[0]!.toUpperCase()).join("") || "?";
  const arrangeable = f.abilityMethod === "standard_array";
  const preview = abilityPreview(f.abilityMethod, f.standardArray);
  return (
    <Flyout title="New character" onClose={onClose}
      footer={<div className="v4-foot"><Button data-testid="create-character" onClick={create}>Create character</Button><Button onClick={onClose}>Cancel</Button></div>}>
      <form className="v4-new" onSubmit={(e) => { e.preventDefault(); create(); }}>
        <div className="v4-portraits" aria-hidden="true">
          <div className="v4-portrait side">B</div><div className="v4-portrait main">{initials}</div><div className="v4-portrait side">R</div>
        </div>
        <Expander flyout title="Character details">
          <p className="sub">Begin with a few basics about your character.</p>
          <label className="v4-fl" id="v4-name-label" htmlFor="v4-name">Name</label>
          <div className="v4-namefield">
            <TextBox light id="v4-name" data-testid="new-character-name" aria-labelledby="v4-name-label" value={f.name} placeholder="Unnamed Hero" data-autofocus onChange={(e) => set("name", e.target.value)} />
            <IconButton icon="dice" label="Random name" onClick={() => set("name", randomName())} />
          </div>
          <div className="v4-row">
            <div><label className="v4-fl" htmlFor="v4-level">Starting level</label>
              <ComboBox light id="v4-level" options={LEVELS} value={String(f.startingLevel)} onChange={(e) => set("startingLevel", Number(e.target.value))} /></div>
            <div><label className="v4-fl" htmlFor="v4-gender">Gender</label>
              <ComboBox light id="v4-gender" options={GENDERS} value={f.gender} onChange={(e) => set("gender", e.target.value)} /></div>
          </div>
          <label className="v4-fl" htmlFor="v4-method">Abilities generation option</label>
          <ComboBox light id="v4-method" options={methodOptions} value={f.abilityMethod} onChange={(e) => { setGrab(null); set("abilityMethod", e.target.value as AbilityMethod); }} />
          <p className="sub hint">{arrangeable ? "Set your ability scores by dragging the values between tiles." : "Ability scores are set on the Abilities tab."}</p>
          <div className="v4-abil" role="group" aria-label="Ability score preview">
            {ABILITIES.map((a, i) => (
              <button key={a} type="button" className="v4-tile-abil" disabled={!arrangeable} aria-pressed={grab === i}
                aria-label={arrangeable ? `${a} ${preview[i]}: drag onto another ability, or press and choose another to swap` : `${a} ${preview[i]}`}
                draggable={arrangeable}
                onDragStart={(e) => { e.dataTransfer.setData("text/plain", String(i)); e.dataTransfer.effectAllowed = "move"; }}
                onDragOver={(e) => { if (arrangeable) { e.preventDefault(); e.dataTransfer.dropEffect = "move"; } }}
                onDrop={(e) => { e.preventDefault(); const from = Number(e.dataTransfer.getData("text/plain")); if (Number.isInteger(from)) swap(from, i); setGrab(null); }}
                onClick={() => tapTile(i)}>
                <b>{a}</b><span>{preview[i]}</span>
              </button>
            ))}
          </div>
        </Expander>
        <section className="aur-expander flyout-variant v4-plain" aria-labelledby="v4-opts-title">
          <div className="aur-expander-head"><h2 className="aur-expander-toggle" id="v4-opts-title"><span className="aur-expander-title">Character options</span></h2></div>
          <div className="aur-expander-body">
          <p className="sub">Switch optional rules on or off for this character.</p>
          <div className="v4-opts">
            {OPTIONS.map((o) => (
              <div className="v4-opt" key={o.key}>
                <label><CheckBox checked={f[o.key]} onChange={(e) => set(o.key, e.target.checked)} />{o.title}</label>
                <p>{o.hint}</p>
              </div>
            ))}
            {contentOptions.map((d) => (
              <div className="v4-opt" key={d.id}>
                <label>
                  <CheckBox checked={f.custom[d.id] ?? d.defaultOn} onChange={(e) => set("custom", { ...f.custom, [d.id]: e.target.checked })} />
                  {d.name}<span className="mg-tag">custom</span>
                </label>
                <p>{d.description}</p>
              </div>
            ))}
            <div className="v4-opt">
              <label className="plain" htmlFor="v4-edition">Rules edition</label>
              <ComboBox light id="v4-edition" options={editionOptions} value={f.ruleset} onChange={(e) => set("ruleset", e.target.value as Ruleset)} />
            </div>
          </div>
          </div>
        </section>
      </form>
    </Flyout>
  );
}
