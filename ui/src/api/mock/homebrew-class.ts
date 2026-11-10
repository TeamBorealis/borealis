import type { HomebrewChildEntry, HomebrewForm, HomebrewKindDescriptor, Ruleset } from "../types";

// The class form's parts the web mock makes the way `borealis-authoring` does (WB-16): a new class with its generated
// features (`new_class_form`) and a new child given by its parent's children field (`add_child`).

const paragraph = (text: string) => ({ type: "doc", content: [{ type: "paragraph", attrs: { joined: false, bare: false }, content: [{ type: "text", text }] }] });

/** A kind's new form: an empty name and every field's starting value (`KindDef::new_form`). */
export function newForm(kind: HomebrewKindDescriptor, key: string): HomebrewForm {
  const starts = Object.fromEntries(kind.fields.filter((f) => f.default !== undefined).map((f) => [f.key, f.default]));
  return { key, kind: kind.id, fields: { name: "", ...starts }, blocks: [], children: [], preserved: [] };
}

/** A new class of `edition` with Ability Score Improvement (4), the subclass choice (3) and, 2024, the Epic Boon (19). */
export function newClassForm(kinds: readonly HomebrewKindDescriptor[], edition: Ruleset): HomebrewForm {
  const classKind = kinds.find((k) => k.id === "class")!;
  const featureKind = kinds.find((k) => k.id === "classFeature")!;
  const newer = edition === "dnd2024";
  const feature = (key: string, name: string, text: string, fields: Record<string, unknown>): HomebrewForm => {
    const f = newForm(featureKind, key);
    return { ...f, fields: { ...f.fields, name, description: paragraph(text), ...fields } };
  };
  const made: [HomebrewForm, number][] = [
    [feature("n1", "Ability Score Improvement", newer
      ? "You gain the Ability Score Improvement feat or another feat of your choice for which you qualify."
      : "You can increase one ability score of your choice by 2, or two ability scores of your choice by 1, or take a feat.",
    { asiLevels: newer ? [4, 8, 12, 16] : [4, 8, 12, 16, 19] }), 4],
    [feature("n2", "Subclass", "You gain a subclass of your choice.", { subclassLabel: "Subclass" }), 3],
    ...(newer ? [[feature("n3", "Epic Boon", "You gain an Epic Boon feat or another feat of your choice for which you qualify.", { epicBoon: true }), 19] as [HomebrewForm, number]] : []),
  ];
  const base = newForm(classKind, "e");
  const levels: HomebrewChildEntry[] = made.map(([c, level]) => ({ level, child: c.key })).sort((a, b) => (a.level ?? 1) - (b.level ?? 1));
  return { ...base, fields: { ...base.fields, edition, levels }, children: made.map(([c]) => c) };
}

/** A key no form of the draft has (`n1`, `n2`...). */
export function newChildKey(form: HomebrewForm): string {
  const used = new Set<string>();
  const walk = (f: HomebrewForm) => { if (f.key) used.add(f.key); f.children.forEach(walk); };
  walk(form);
  for (let n = 1; ; n += 1) if (!used.has(`n${n}`)) return `n${n}`;
}

/** `parent` with `child` last and listed in the parent's children field for its type (`add_child`); null when the
 * parent's form has no place for it. */
export function addChild(parent: HomebrewForm, child: HomebrewForm, kinds: readonly HomebrewKindDescriptor[]): HomebrewForm | null {
  const ck = kinds.find((k) => k.id === child.kind);
  const pk = kinds.find((k) => k.id === parent.kind);
  const type = ck?.elementTypes[0];
  if (!ck || !pk || !type || !ck.parentKinds.includes(pk.id)) return null;
  const field = pk.fields.find((f) => ["levels", "children", "package"].includes(f.control) && (f.picker?.types ?? []).includes(type));
  if (!field) return null;
  const list = Array.isArray(parent.fields[field.key]) ? parent.fields[field.key] as HomebrewChildEntry[] : [];
  const entry: HomebrewChildEntry = field.control === "levels" ? { level: 1, child: child.key } : { child: child.key };
  const entries = field.control === "package" ? [entry] : [...list, entry];
  return { ...parent, fields: { ...parent.fields, [field.key]: entries }, children: [...parent.children, child] };
}
