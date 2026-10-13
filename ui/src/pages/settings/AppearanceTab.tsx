import { mdiSnake } from "@mdi/js";
import type { AccentColor, AppSettings, ThemeMode } from "../../api/contract";
import { Icon } from "../../components/Icon";
import { ACCENTS } from "../../lib/accent";
import { FieldLabel, SectionDesc } from "./rows";

const THEMES: { id: Exclude<ThemeMode, "system">; label: string; color: string; glyph: string }[] = [
  { id: "light", label: "Light", color: "#f7f7f7", glyph: "#161616" },
  { id: "dark", label: "Dark", color: "#161616", glyph: "#f7f7f7" },
];
const PORTRAIT_SIZES = [125, 150, 175];

function Swatch({ label, color, glyph, selected, size, mark, onPick }: { label: string; color: string; glyph: string; selected: boolean; size: number; mark?: "dragon"; onPick: () => void }) {
  return (
    <button type="button" role="radio" aria-checked={selected} aria-label={label} title={label} className="aur-swatch" style={{ background: color, color: glyph }} onClick={onPick}>
      {mark === "dragon" ? (
        <svg className="icon dragon" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={mdiSnake} fill="currentColor" /></svg>
      ) : selected && <span style={{ width: size, height: size }}><Icon name="check" /></span>}
    </button>
  );
}

/** APPEARANCE: accent swatches and theme swatches preview live; the slider sets the collection portrait size. */
export function AppearanceTab({ form, set }: { form: AppSettings; set: (patch: Partial<AppSettings>) => void }) {
  // "system" has no stylesheet of its own: the app shows its default (dark) theme, so the Dark swatch is the marked one.
  const theme = form.theme === "light" ? "light" : "dark";
  const size = PORTRAIT_SIZES.includes(form.portraitSize) ? form.portraitSize : 150;
  return (
    <>
      <h3 className="aur-set-title13 first flush">APPEARANCE</h3>
      <SectionDesc>Customize the appearance of the application.</SectionDesc>
      <FieldLabel>SELECT YOUR ACCENT COLOR</FieldLabel>
      <div className="aur-swatches" role="radiogroup" aria-label="Accent color">
        {ACCENTS.map((a) => (
          <Swatch key={a.id} label={a.label} color={a.color} glyph="#ffffff" size={20} selected={form.accentColor === a.id} onPick={() => set({ accentColor: a.id as AccentColor })} />
        ))}
      </div>
      <FieldLabel>SELECT YOUR THEME COLOR</FieldLabel>
      <div className="aur-swatches theme" role="radiogroup" aria-label="Theme color">
        {THEMES.map((t) => (
          <Swatch key={t.id} label={t.label} color={t.color} glyph={t.glyph} size={28} mark="dragon" selected={theme === t.id} onPick={() => set({ theme: t.id })} />
        ))}
      </div>
      <label className="aur-set-field slider" htmlFor="set-portrait">CHARACTER COLLECTION PORTRAIT SIZE ({size}px)</label>
      <input id="set-portrait" className="aur-slider" type="range" min={125} max={175} step={25} value={size}
        onChange={(e) => set({ portraitSize: Number(e.target.value) })} />
      <div className="aur-slider-ticks" aria-hidden="true">{PORTRAIT_SIZES.map((n) => <i key={n} />)}</div>
    </>
  );
}
