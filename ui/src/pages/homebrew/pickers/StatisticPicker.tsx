import { useMemo, useState } from "react";
import type { HomebrewStatEntry, HomebrewStatGroup } from "../../../api/contract";
import { Button, ComboBox, Dialog, Expander, Icon, NumericUpDown, SearchInput } from "../../../components";
import { type CreatorEnv, classChoices, className, useCreatorEnv } from "../env";

const CLASS = "{class}";
const N = "{n}";

const escapeRe = (s: string): string => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** A class-scoped pattern as a matcher: `{class}` any name, `{n}` a whole number. */
function patternRe(pattern: string): RegExp {
  const src = escapeRe(pattern).replace(escapeRe(CLASS), "(?<cls>.+?)").replace(escapeRe(N), "(?<n>\\d+)");
  return new RegExp(`^${src}$`, "i");
}

/** A pattern's entry filled in: "Spell slots of level 3 (Paladin)". */
function filledLabel(entry: HomebrewStatEntry, cls: string | undefined, n: string | undefined, env: CreatorEnv): string {
  const base = n ? entry.label.replace(/\bN\b/, n) : entry.label;
  return cls ? `${base} (${className(env, cls)})` : base;
}

/** The plain words of a statistic name (spec 5.2); a name no catalog entry knows is shown as its words. */
export function statLabel(env: CreatorEnv, name: string): string {
  if (!name) return "";
  for (const g of env.stats) {
    for (const s of g.stats) {
      if (!s.classScoped) { if (s.name.toLowerCase() === name.toLowerCase()) return s.label; continue; }
      const m = patternRe(s.name).exec(name);
      if (m) return filledLabel(s, m.groups?.cls, m.groups?.n, env);
    }
  }
  return name.replace(/[:_]/g, " ").replace(/\s+/g, " ").trim();
}

/** Writes a class-scoped pattern's name: the class name in lower case, the number as digits (never typed, spec 5.2). */
export const fillPattern = (pattern: string, cls: string, n: number): string => pattern.replace(CLASS, cls.toLowerCase()).replace(N, String(n));

function matches(entry: HomebrewStatEntry, words: string[]): boolean {
  const hay = entry.label.toLowerCase();
  return words.every((w) => hay.includes(w));
}

/** The class and number a class-scoped statistic needs, chosen from lists (spec 5.2). */
function PatternStep({ entry, onDone, onBack }: { entry: HomebrewStatEntry; onDone: (name: string) => void; onBack: () => void }) {
  const env = useCreatorEnv();
  const classes = classChoices(env);
  const [cls, setCls] = useState(classes[0]?.value ?? "");
  const [n, setN] = useState(1);
  const needsClass = entry.name.includes(CLASS);
  const needsN = entry.name.includes(N);
  return (
    <div className="hb-stat-step">
      <p className="hb-stat-step-title">{entry.label}</p>
      {needsClass && (
        <div className="mg-f">
          <label htmlFor="hb-stat-class">Class</label>
          {classes.length > 0
            ? <ComboBox id="hb-stat-class" value={cls} onChange={setCls} options={classes} />
            : <p className="hb-field-help">No class is loaded.</p>}
        </div>
      )}
      {needsN && (
        <div className="mg-f">
          <label>Level</label>
          <NumericUpDown label="Level" value={n} min={1} max={9} onChange={setN} />
        </div>
      )}
      <div className="hb-row-actions">
        <Button onClick={onBack}>Back</Button>
        <Button variant="primary" disabled={needsClass && !cls} onClick={() => onDone(fillPattern(entry.name, cls, n))}>Choose</Button>
      </div>
    </div>
  );
}

/** The statistic picker dialog (spec 5.2): statistics grouped in plain words, a search, and the class step for class-scoped ones. */
export function StatisticPicker({ title, onChoose, onClose }: { title: string; onChoose: (name: string) => void; onClose: () => void }) {
  const env = useCreatorEnv();
  const [query, setQuery] = useState("");
  const [pattern, setPattern] = useState<HomebrewStatEntry | null>(null);
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const groups = useMemo<HomebrewStatGroup[]>(
    () => env.stats.map((g) => ({ ...g, stats: g.stats.filter((s) => matches(s, words)) })).filter((g) => g.stats.length > 0),
    [env.stats, words.join(" ")], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const pick = (s: HomebrewStatEntry) => (s.classScoped ? setPattern(s) : onChoose(s.name));
  return (
    <Dialog title={`Choose: ${title}`} onClose={onClose} className="hb-stat-picker" footer={<Button onClick={onClose}>Cancel</Button>}>
      {pattern ? <PatternStep entry={pattern} onDone={onChoose} onBack={() => setPattern(null)} /> : (
        <>
          <SearchInput label="Search statistics" value={query} onChange={setQuery} data-autofocus />
          {env.stats.length === 0 && <p className="pick-empty">The statistics are loading.</p>}
          <div className="hb-stat-groups">
            {groups.map((g) => (
              <Expander key={`${g.id}-${words.length > 0}`} title={g.label.toUpperCase()} defaultOpen={words.length > 0 || groups.length <= 2}>
                <ul className="hb-stat-list" aria-label={g.label}>
                  {g.stats.map((s) => (
                    <li key={s.name}>
                      <button type="button" className="hb-stat-row" onClick={() => pick(s)}>
                        <span>{s.label}</span>{s.classScoped && <span className="hb-stat-more" aria-hidden="true"><Icon name="chevron" /></span>}
                      </button>
                    </li>
                  ))}
                </ul>
              </Expander>
            ))}
          </div>
        </>
      )}
    </Dialog>
  );
}

/** A statistic field: its plain words and a Pick button (spec 5.2). The name is never shown or typed. */
export function StatisticField({ id, label, value, onChange, describedBy, invalid, clearable = true }: {
  id?: string; label: string; value: string; onChange: (name: string) => void; describedBy?: string; invalid?: boolean; clearable?: boolean;
}) {
  const env = useCreatorEnv();
  const [open, setOpen] = useState(false);
  return (
    <div className={`hb-ref ${invalid ? "hb-invalid" : ""}`} id={id} aria-describedby={describedBy}>
      {value && (
        <span className="hb-chip-item">
          <span className="hb-ref-name">{statLabel(env, value)}</span>
          {clearable && <button type="button" className="hb-chip-x" aria-label={`Clear ${label}`} onClick={() => onChange("")}><Icon name="x" /></button>}
        </span>
      )}
      <button type="button" className="hb-pick-btn" aria-label={`${value ? "Change" : "Pick"}: ${label}`} onClick={() => setOpen(true)}>
        <Icon name="search" />{value ? "Change" : "Pick"}
      </button>
      {open && <StatisticPicker title={label} onClose={() => setOpen(false)} onChoose={(name) => { setOpen(false); onChange(name); }} />}
    </div>
  );
}
