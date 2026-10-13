import { MODERN_LIST_ADD_BELOW, MODERN_LIST_REORDER_DOWN, MODERN_LIST_REORDER_UP, MODERN_PAGE_FILE_PDF_TAG, type OpenIcon } from "../../components/controls/openIcons";
import { mdiCheck, mdiCheckboxMultipleMarked, mdiDiceMultiple, mdiTagHeart, mdiTagMinus, mdiCog, mdiFileFind, mdiFormatListBulleted, mdiMinus, mdiPencil, mdiPlus } from "@mdi/js";

/** Attack-card remove glyph: a 13x3 bar (classic builder), centred in the 24 box. */
const CARD_MINUS = "M5.5 10.5h13v3h-13z";

const PATHS = {
  list: mdiFormatListBulleted, edit: mdiPencil, minus: mdiMinus, cardMinus: CARD_MINUS,
  plus: mdiPlus, dice: mdiDiceMultiple, tagHeart: mdiTagHeart, tagMinus: mdiTagMinus, checkAll: mdiCheckboxMultipleMarked, check: mdiCheck, cog: mdiCog, preview: mdiFileFind,
} as const;
/** Open-licensed (MahApps.IconPacks Modern, MIT) glyphs: own viewBox and CSS box matching the ink. */
const OPEN = {
  addAttack: { icon: MODERN_LIST_ADD_BELOW, w: 16, h: 13, margin: "-4px -1px 0 2px" },
  moveUp: { icon: MODERN_LIST_REORDER_UP, w: 17, h: 13 },
  moveDown: { icon: MODERN_LIST_REORDER_DOWN, w: 17, h: 13 },
  pdf: { icon: MODERN_PAGE_FILE_PDF_TAG, w: 14, h: 17 },
} as const satisfies Record<string, { icon: OpenIcon; w: number; h: number; margin?: string }>;
export type MgIconName = keyof typeof PATHS | keyof typeof OPEN;

/** Open-set (Material Design Icons) glyphs used by the Manage and Sheet screens. */
export function MgIcon({ name }: { name: MgIconName }) {
  if (name in OPEN) {
    const { icon, w, h, margin } = OPEN[name as keyof typeof OPEN] as { icon: OpenIcon; w: number; h: number; margin?: string };
    return <svg className="icon" viewBox={icon.viewBox} style={{ width: w, height: h, margin }} aria-hidden focusable="false"><path d={icon.d} /></svg>;
  }
  return <svg className="icon" viewBox="0 0 24 24" aria-hidden focusable="false"><path d={PATHS[name as keyof typeof PATHS]} /></svg>;
}
