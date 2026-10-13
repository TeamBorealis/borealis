/**
 * Wire shape of mock slot ids. The real backend encodes a slot as `owner:rule:ordinal[@instance]`
 * (owner = the element that grants the choice, e.g. `ID_2024_CLASS_FEATURE_WIZARD_SPELLCASTING:38:1`).
 * The mock keys its choices by readable internal keys (`c0:cantrip:0`, `bg:asi:2`) and only ever
 * emits the encoded form, so UI code cannot come to depend on the internal layout.
 */
const OWNER_PREFIX = "ID_MOCK_";

/** Internal key -> wire id in the real `owner:rule:ordinal` shape. */
export function toWireSlotId(key: string): string {
  const parts = key.split(":");
  const last = parts.at(-1) ?? "";
  const numbered = parts.length > 1 && /^\d+$/.test(last);
  const head = numbered ? parts.slice(0, -1) : parts;
  const owner = OWNER_PREFIX + head.join("_").toUpperCase().replace(/[^A-Z0-9_]/g, "_");
  return numbered ? `${owner}:1:${Number(last) + 1}` : `${owner}:0:1`;
}

/** The def whose wire id is `wireId` (undefined for unknown ids, including raw internal keys). */
export const findByWireId = <T extends { id: string }>(defs: T[], wireId: string): T | undefined =>
  defs.find((d) => toWireSlotId(d.id) === wireId);
