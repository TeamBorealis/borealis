import { mdiArrowDown, mdiArrowUp, mdiCheck, mdiCheckCircle, mdiClose, mdiCloseBox, mdiFilter, mdiLink, mdiMapMarker, mdiPackageUp, mdiPencil } from "@mdi/js";

const PATHS = { check: mdiCheckCircle, filter: mdiFilter, link: mdiLink, pin: mdiMapMarker, extract: mdiPackageUp, details: mdiPencil, up: mdiArrowUp, down: mdiArrowDown, del: mdiCloseBox, x: mdiClose, tick: mdiCheck } as const;
export type GlyphName = keyof typeof PATHS;

/** Open-set (Material Design Icons) glyph used by the classic-builder-style equipment grids and action row. */
export function Glyph({ name, label }: { name: GlyphName; label?: string }) {
  return (
    <svg className="eq-glyph" viewBox="0 0 24 24" role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true} focusable="false">
      <path d={PATHS[name]} />
    </svg>
  );
}
