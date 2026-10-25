import { useId } from "react";
import type { SheetSettings } from "../../api/contract";
import type { SheetEditionOption, SheetTheme } from "../../api/types/settings";
import { ComboBox } from "../../components";
import { DescCheck, FieldLabel, Help, SectionDesc, SectionTitle } from "./rows";
import { PhoneCase } from "../../lib/phoneCase";

const THEMES: { value: SheetTheme; label: string }[] = [{ value: "colour", label: "Colour" }, { value: "ink", label: "Ink" }];
const EDITIONS: { value: SheetEditionOption; label: string }[] = [
  { value: "auto", label: "Automatic (from character)" }, { value: "2014", label: "2014" }, { value: "2024", label: "2024" },
];

/** Approximate PDF sizes; edit here once re-measured. */
export const FILLABLE_SIZE_NOTE = "About 0.4 MB instead of 0.2 MB.";
export const FILLABLE_TOOLTIP = `Keeps the blank play fields (hit points, notes, coins, checkboxes) editable in the PDF. Values from Borealis are printed. ${FILLABLE_SIZE_NOTE}`;

type Cards = SheetSettings["cards"];

const NEW_PAGE: Partial<Record<keyof Cards, { key: keyof SheetSettings["cardsNewPage"]; title: string }>> = {
  item: { key: "item", title: "Start Item Cards on a New Page" },
  attack: { key: "attack", title: "Start Attack Cards on a New Page" },
  feature: { key: "feature", title: "Start Feature Cards on a New Page" },
};

const CARDS: { key: keyof Cards; title: string; desc: string }[] = [
  { key: "spell", title: "Spell cards", desc: "A description of the spell as well as the casting time, range, duration, and required components." },
  { key: "item", title: "Item cards", desc: "A description, details, and added notes." },
  { key: "attack", title: "Attack Cards", desc: "Detailed attack cards for your weapon(s). Not to be confused with item cards for your weapons." },
  { key: "feature", title: "Feature Cards", desc: "Detailed description for features such as racial traits, class and archetype features, and feats." },
];

/** CHARACTER SHEET tab, rows in the classic builder's order. */
export function SheetTab({ sheet, onChange }: { sheet: SheetSettings; onChange: (next: SheetSettings) => void }) {
  const set = (patch: Partial<SheetSettings>) => onChange({ ...sheet, ...patch });
  const themeId = useId();
  const editionId = useId();
  return (
    <>
      <h3 className="aur-set-title13 first flush"><PhoneCase>CHARACTER SHEET</PhoneCase></h3>
      <SectionDesc>Manage some aspects of the output of your character sheet.</SectionDesc>
      <DescCheck title="Form Fillable" description="Fields on the sheet can be edited in a PDF viewer. Off by default: the PDF is flattened so values are printed and not editable." tooltip={FILLABLE_TOOLTIP} checked={sheet.formFillable} onChange={(v) => set({ formFillable: v })} />
      <FieldLabel htmlFor={themeId}>THEME</FieldLabel>
      <ComboBox id={themeId} value={sheet.theme} options={THEMES} onChange={(e) => set({ theme: e.target.value as SheetTheme })} />
      <Help>Ink is monochrome and printer-friendly.</Help>
      <FieldLabel htmlFor={editionId}>RULES EDITION</FieldLabel>
      <ComboBox id={editionId} value={sheet.edition} options={EDITIONS} onChange={(e) => set({ edition: e.target.value as SheetEditionOption })} />
      <Help>Automatic uses the ruleset of the character.</Help>
      <DescCheck title="Style Formatting (Recommended)" description="Apply some styling for certain fields such as Features & Traits to improve readability. This option can be used in combination with form-fillable." checked={sheet.styleFormatting} onChange={(v) => set({ styleFormatting: v })} />
      <DescCheck title="Invert Ability Score" description="This will switch the location of the ability score and modifier. This includes the companion page if you have it." checked={sheet.swapAbilityPositions} onChange={(v) => set({ swapAbilityPositions: v })} />
      <SectionTitle>SPELLCASTING PAGE</SectionTitle>
      <SectionDesc>Options specific to the spellcasting page.</SectionDesc>
      <DescCheck title="Include Non-Prepared Spells" description="Also include spells you have not prepared on the spellcasting page. With the dynamic spellcasting page there is room for all the unprepared spells." checked={sheet.includeNonPreparedSpells} onChange={(v) => set({ includeNonPreparedSpells: v })} />
      <SectionTitle>GENERATED CARDS</SectionTitle>
      <SectionDesc>Include some of the additional custom generated cards.</SectionDesc>
      {CARDS.map((c) => {
        const sub = NEW_PAGE[c.key];
        return (
          <div key={c.key}>
            <DescCheck title={c.title} description={c.desc} checked={sheet.cards[c.key]} onChange={(v) => set({ cards: { ...sheet.cards, [c.key]: v } })} />
            {sub && sheet.cards[c.key] && (
              <DescCheck sub title={sub.title} checked={sheet.cardsNewPage[sub.key]} onChange={(v) => set({ cardsNewPage: { ...sheet.cardsNewPage, [sub.key]: v } })} />
            )}
          </div>
        );
      })}
      <SectionTitle>LEGACY</SectionTitle>
      <SectionDesc>While the Borealis sheets are not yet finalized the legacy pages are still available should you choose to use it.</SectionDesc>
      <DescCheck title="Legacy Spellcasting Page" description="Use the single page basic spellcasting page layout. It's best not to use the non-prepared spells option when using this option due to limited space for spells." checked={sheet.legacySpellcastingPage} onChange={(v) => set({ legacySpellcastingPage: v })} />
    </>
  );
}
