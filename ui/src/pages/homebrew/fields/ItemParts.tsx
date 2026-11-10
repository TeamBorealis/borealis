import type { HomebrewCoins, HomebrewContentsRow, HomebrewPackageItem, HomebrewPackageOption, HomebrewPickerTarget, HomebrewRef } from "../../../api/contract";
import { Icon, NumericUpDown, TextBox } from "../../../components";
import { RefField } from "../pickers/RefField";

/** What a pack or a starting-equipment option can hold (spec 4.9). */
export const ITEM_TARGET: HomebrewPickerTarget = { types: ["Item", "Weapon", "Armor", "Magic Item", "Item Pack"] };

/** The most of one item a pack or an option lists. */
const MAX_AMOUNT = 999;
/** The most coins of one kind an option gives. */
const MAX_COINS = 99_999;
const COINS: { key: keyof HomebrewCoins; label: string }[] = [
  { key: "cp", label: "Copper (cp)" }, { key: "sp", label: "Silver (sp)" }, { key: "ep", label: "Electrum (ep)" },
  { key: "gp", label: "Gold (gp)" }, { key: "pp", label: "Platinum (pp)" },
];
const NO_COINS: HomebrewCoins = { cp: 0, sp: 0, ep: 0, gp: 0, pp: 0 };

const one = (r: HomebrewRef | null): HomebrewRef[] => (r ? [r] : []);
const replaceAt = <T,>(list: readonly T[], i: number, item: T): T[] => list.map((x, j) => (j === i ? item : x));
const without = <T,>(list: readonly T[], i: number): T[] => list.filter((_, j) => j !== i);

/** The rows a `contents` value holds (anything else reads as no rows). */
export function contentsOf(value: unknown): HomebrewContentsRow[] {
  return Array.isArray(value) ? value.filter((r): r is HomebrewContentsRow => typeof r === "object" && r !== null && "amount" in r) : [];
}

/** The options a `packageOptions` value holds. */
export function optionsOf(value: unknown): HomebrewPackageOption[] {
  return Array.isArray(value) ? value.filter((o): o is HomebrewPackageOption => typeof o === "object" && o !== null && "label" in o) : [];
}

/** The next free option letter: A, B, C... */
export function nextLetter(options: readonly HomebrewPackageOption[]): string {
  const used = new Set(options.map((o) => o.label.trim().toUpperCase()));
  for (let c = 65; c <= 90; c += 1) if (!used.has(String.fromCharCode(c))) return String.fromCharCode(c);
  return "";
}

/**
 * "Items in the pack" (spec 4.9, Item Pack `extract`): one row per item, picked with the reference picker, with how many.
 * Laid out like the Attacks list rows of Manage (13.6): the picked name, a compact spinner, X to remove, then Add.
 */
export function ContentsField({ id, label, value, onChange }: { id?: string; label: string; value: readonly HomebrewContentsRow[]; onChange: (rows: HomebrewContentsRow[]) => void }) {
  return (
    <div className="hb-bf-rows hb-item-rows" id={id} role="group" aria-label={label}>
      {value.map((row, i) => (
        <div key={i} className="hb-bf-row" role="group" aria-label={`Item ${i + 1}`}>
          <RefField label={`Item ${i + 1}`} target={ITEM_TARGET} value={one(row.item)} onChange={(r) => onChange(replaceAt(value, i, { ...row, item: r[0] ?? null }))} />
          <span className="hb-row-controls">
            <span className="hb-inline-label">How many</span>
            <NumericUpDown label={`Item ${i + 1}: how many`} value={row.amount} min={1} max={MAX_AMOUNT} compact onChange={(n) => onChange(replaceAt(value, i, { ...row, amount: n }))} />
          </span>
          <button type="button" className="hb-chip-x" aria-label={`Remove item ${i + 1}`} onClick={() => onChange(without(value, i))}><Icon name="x" /></button>
        </div>
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => onChange([...value, { item: null, amount: 1 }])}><Icon name="plus" />Add an item</button>
    </div>
  );
}

function PackageItemRow({ item, name, onChange, onRemove }: { item: HomebrewPackageItem; name: string; onChange: (item: HomebrewPackageItem, typing: boolean) => void; onRemove: () => void }) {
  return (
    <div className="hb-bf-row" role="group" aria-label={name}>
      <span className="hb-row-controls hb-wrap">
        <RefField label={name} target={ITEM_TARGET} value={one(item.item)} onChange={(r) => onChange({ ...item, item: r[0] ?? null }, false)} />
        {!item.item && (
          <TextBox aria-label={`${name}: or name a generic item`} placeholder="Or name a generic item" value={item.name}
            onChange={(e) => onChange({ ...item, name: e.target.value }, true)} />
        )}
        <span className="hb-inline-label">How many</span>
        <NumericUpDown label={`${name}: how many`} value={item.qty} min={1} max={MAX_AMOUNT} compact onChange={(n) => onChange({ ...item, qty: n }, false)} />
        <TextBox aria-label={`${name}: note`} placeholder="Note, like 10 sheets" value={item.note} onChange={(e) => onChange({ ...item, note: e.target.value }, true)} />
      </span>
      <button type="button" className="hb-chip-x" aria-label={`Remove ${name.toLowerCase()}`} onClick={onRemove}><Icon name="x" /></button>
    </div>
  );
}

function OptionCard({ option, onChange, onRemove }: { option: HomebrewPackageOption; onChange: (o: HomebrewPackageOption, typing: boolean) => void; onRemove: () => void }) {
  const title = `Option ${option.label.trim() || "?"}`;
  const coins = option.coins ?? NO_COINS;
  return (
    <fieldset className="hb-package-option" aria-label={title}>
      <legend className="hb-package-title">
        <span>{title}</span>
        <button type="button" className="hb-chip-x" aria-label={`Remove ${title.toLowerCase()}`} onClick={onRemove}><Icon name="x" /></button>
      </legend>
      <div className="hb-row-controls hb-package-letter">
        <span className="hb-inline-label">Letter</span>
        <TextBox aria-label={`${title}: letter`} value={option.label} maxLength={2} onChange={(e) => onChange({ ...option, label: e.target.value.toUpperCase() }, true)} />
      </div>
      <div className="hb-bf-rows">
        {option.items.map((it, i) => (
          <PackageItemRow key={i} item={it} name={`${title}, item ${i + 1}`} onRemove={() => onChange({ ...option, items: without(option.items, i) }, false)}
            onChange={(next, typing) => onChange({ ...option, items: replaceAt(option.items, i, next) }, typing)} />
        ))}
        <button type="button" className="hb-pick-btn" onClick={() => onChange({ ...option, items: [...option.items, { item: null, name: "", qty: 1, note: "" }] }, false)}>
          <Icon name="plus" />Add an item
        </button>
      </div>
      <div className="hb-row-controls hb-wrap hb-package-coins" role="group" aria-label={`${title}: coins`}>
        {COINS.map((c) => (
          <span key={c.key} className="hb-row-controls">
            <span className="hb-inline-label">{c.label}</span>
            <NumericUpDown label={`${title}: ${c.label}`} value={coins[c.key]} min={0} max={MAX_COINS} compact onChange={(n) => onChange({ ...option, coins: { ...coins, [c.key]: n } }, false)} />
          </span>
        ))}
      </div>
    </fieldset>
  );
}

/**
 * "Options" of a starting-equipment package (spec 4.9, `data.options`): lettered options (A, B...), each with its items
 * (picked, or a generic item in words, with how many and a note) and the coins it gives. The player takes one.
 */
export function PackageOptionsField({ id, label, value, onChange }: { id?: string; label: string; value: readonly HomebrewPackageOption[]; onChange: (options: HomebrewPackageOption[], typing: boolean) => void }) {
  return (
    <div className="hb-package" id={id} role="group" aria-label={label}>
      {value.map((o, i) => (
        <OptionCard key={i} option={o} onRemove={() => onChange(without(value, i), false)} onChange={(next, typing) => onChange(replaceAt(value, i, next), typing)} />
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => onChange([...value, { label: nextLetter(value), items: [], coins: { ...NO_COINS } }], false)}>
        <Icon name="plus" />Add an option
      </button>
    </div>
  );
}
