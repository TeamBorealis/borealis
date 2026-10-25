import { useId } from "react";
import type { AbilityMethod, AppSettings, RowHeight } from "../../api/contract";
import { ComboBox, TextBox } from "../../components";
import { DescCheck, FieldLabel, Help, SectionDesc, SectionTitle } from "./rows";
import { PhoneCase } from "../../lib/phoneCase";

type Props = { form: AppSettings; set: (patch: Partial<AppSettings>) => void };

const METHODS: { value: AbilityMethod; label: string }[] = [
  { value: "roll_3d6", label: "Roll 3d6" }, { value: "roll_4d6", label: "Roll 4d6 Discard Lowest" },
  { value: "standard_array", label: "Standard Array" }, { value: "point_buy", label: "Point Buy System" },
];
const ROW_HEIGHTS: { value: RowHeight; label: string }[] = [
  { value: "small", label: "Small" }, { value: "medium", label: "Medium (Default)" }, { value: "large", label: "Large" },
];

function AbilityMethodField({ form, set }: Props) {
  const id = useId();
  // Borealis can hold "manual"; the classic builder's dialog does not list it, so it only appears while it is the stored value.
  const options = form.abilityMethod === "manual" ? [...METHODS, { value: "manual" as const, label: "Manual" }] : METHODS;
  return (
    <>
      <FieldLabel htmlFor={id}>ABILITY SCORE GENERATION OPTION</FieldLabel>
      <ComboBox id={id} value={form.abilityMethod} options={options} onChange={(e) => set({ abilityMethod: e.target.value as AbilityMethod })} />
    </>
  );
}

export function GeneralTab({ form, set }: Props) {
  const name = useId();
  const rows = useId();
  return (
    <>
      <h3 className="aur-set-title13 first flush"><PhoneCase>APPLICATION SETTINGS</PhoneCase></h3>
      <SectionDesc>The behaviour of the application and its UI elements.</SectionDesc>
      <FieldLabel htmlFor={name}>PLAYER NAME</FieldLabel>
      <TextBox id={name} value={form.playerName} onChange={(e) => set({ playerName: e.target.value })} />
      <AbilityMethodField form={form} set={set} />
      <SectionTitle>SELECTION PICKERS</SectionTitle>
      <FieldLabel htmlFor={rows}>ROW HEIGHT</FieldLabel>
      <ComboBox id={rows} value={form.rowHeight} options={ROW_HEIGHTS} onChange={(e) => set({ rowHeight: e.target.value as RowHeight })} />
      <Help>Reduce the height of the rows to fit more content on your screen.</Help>
      <DescCheck spaced title="Selection Flow" description="Automatically navigate to the next available selection. Alternatively, toggle this behaviour from the status bar." checked={form.selectionFlow} onChange={(v) => set({ selectionFlow: v })} />
      <SectionTitle>CHARACTER INFORMATION PANEL</SectionTitle>
      <SectionDesc>Click the character portrait at the top of the main window to open the character information panel.</SectionDesc>
      <DescCheck title="Skills Panel Expanded" description="Expand by default when the application loads." checked={form.skillsPanelExpanded} onChange={(v) => set({ skillsPanelExpanded: v })} />
      <DescCheck title="Saving Throws Expanded" description="Expand by default when the application loads." checked={form.savingThrowsExpanded} onChange={(v) => set({ savingThrowsExpanded: v })} />
      <SectionTitle>CHARACTER ADVANCEMENT</SectionTitle>
      <DescCheck title="'Remove Level' Confirmation" description="Display a confirmation box when removing the last level of your character." checked={form.confirmRemoveLevel} onChange={(v) => set({ confirmRemoveLevel: v })} />
    </>
  );
}

export function BuildTab({ form, set }: Props) {
  return (
    <>
      <h3 className="aur-set-title13 first flush"><PhoneCase>BUILD</PhoneCase></h3>
      <SectionDesc>The behaviour of character creation.</SectionDesc>
      <div className="aur-set-gap"><AbilityMethodField form={form} set={set} /></div>
      <DescCheck title="Ability Score Maximum (default)" description="You ability score can't exceed the maximum of 20. This can still be increased by certain features or items." checked={form.abilityCapEnabled} onChange={(v) => set({ abilityCapEnabled: v })} />
    </>
  );
}

export function EquipmentTab({ form, set }: Props) {
  return (
    <>
      <h3 className="aur-set-title13 first flush"><PhoneCase>EQUIPMENT</PhoneCase></h3>
      <SectionDesc>General behaviour of the equipment section and your inventory.</SectionDesc>
      <DescCheck title="'Add Weapon to Attacks' Confirmation" description="Display a confirmation box when equipping a weapon that is not included in your 'Attacks' section to add it." checked={form.confirmEquipWeaponNotInAttacks} onChange={(v) => set({ confirmEquipWeaponNotInAttacks: v })} />
    </>
  );
}
