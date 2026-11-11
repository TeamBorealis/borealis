import type { ReactNode } from "react";
import type { HomebrewFieldSpec, HomebrewForm, HomebrewKindDescriptor, HomebrewProblem } from "../../../api/contract";
import { Button, Dialog, NumericUpDown } from "../../../components";
import { PreservedList } from "../blocks/BlockDialog";
import { FieldView } from "../FieldView";
import { editionNote, holdsValue, otherEdition } from "../editions";
import { childProblems, levelOfChild, nameOf, withChild, withChildLevel } from "./children";

/** What the class (or any parent) form lends its child editors: the form, its edits and the backend's child maker. */
export interface ChildEditing {
  form: HomebrewForm;
  /** Every kind (the child's descriptor). */
  kinds: readonly HomebrewKindDescriptor[];
  problems: readonly HomebrewProblem[];
  /** The form's edition (`dnd2014`, `dnd2024`; null = the pack's). */
  edition: string | null;
  /** One undoable edit of the form; `group` merges typing (spec 10.2). */
  edit: (next: (form: HomebrewForm) => HomebrewForm, group?: string | null) => void;
  /** A new child through the backend (`homebrew_new_draft {parentId}`); `place` adjusts it and its entry in the same
   * undo step. `parentKey` makes it inside that child form of the draft (nested children: a variant's own feature).
   * Resolves with the child's key, null when it could not be made. */
  newChild: (kind: string, place?: (form: HomebrewForm, key: string) => HomebrewForm, parentKey?: string) => Promise<string | null>;
  /** Asks before the child `key` is removed when local characters use its saved element (the save deletes it, spec
   * 10.3); resolves true to go on. Absent: nothing to ask. */
  confirmRemove?: (key: string, name: string) => Promise<boolean>;
}

/** The editing of the child `childKey` of `editing`'s form, for the children it has itself (a background variant's own
 * feature, a racial trait's lineage options): its edits go into the parent's draft as one undo step each, its new
 * children are made inside it by the backend (`parentKey`). */
export function nestedEditing(editing: ChildEditing, childKey: string): ChildEditing | null {
  const index = editing.form.children.findIndex((c) => c.key === childKey);
  const child = editing.form.children[index];
  if (!child) return null;
  const inChild = (form: HomebrewForm, next: (c: HomebrewForm) => HomebrewForm): HomebrewForm => {
    const c = form.children.find((x) => x.key === childKey);
    return c ? withChild(form, childKey, next(c)) : form;
  };
  return {
    form: child,
    kinds: editing.kinds,
    problems: childProblems(editing.problems, index),
    edition: editing.edition,
    edit: (next, group) => editing.edit((form) => inChild(form, next), group),
    newChild: (kind, place, parentKey) =>
      editing.newChild(kind, place ? (form, key) => inChild(form, (c) => place(c, key)) : undefined, parentKey ?? childKey),
    ...(editing.confirmRemove ? { confirmRemove: editing.confirmRemove } : {}),
  };
}

/** Removes the child `key` with `remove` after asking when characters use it (spec 10.3); resolves whether it went. */
export async function removeChecked(editing: ChildEditing, key: string, name: string, remove: () => void): Promise<boolean> {
  if (editing.confirmRemove && !(await editing.confirmRemove(key, name))) return false;
  remove();
  return true;
}

/** The kind of a parent's children of element type `type`. */
export function childKind(editing: ChildEditing, type: string | undefined): HomebrewKindDescriptor | undefined {
  const parent = editing.form.kind;
  return editing.kinds.find((k) => k.parentKinds.includes(parent) && (type === undefined || k.elementTypes.includes(type)));
}

/** The parts of a child the editor shows (spec 4.5 Feature editor: name, level gained, sheet text, rule blocks,
 * description), with the summary line pickers show; a package's lettered options. */
const CHILD_FIELDS: ReadonlySet<string> = new Set(["name", "summary", "rules", "description", "sheet", "options"]);

/** Fields the child editor shows: its own children (a variant's feature, a trait's lineage options), the generated parts
 * only on the feature that has them, edition-limited ones for their edition or while they hold a value (shown with a
 * note, see `otherEdition`). */
function shown(f: HomebrewFieldSpec, child: HomebrewForm, edition: string | null): boolean {
  if (f.expert || f.readOnly) return false;
  if (!CHILD_FIELDS.has(f.key) && f.section !== "Made by the class" && f.control !== "children") return false;
  if (otherEdition(f, edition) && !holdsValue(f, child.fields[f.key])) return false;
  if (f.section === "Made by the class") {
    const v = child.fields[f.key];
    return Array.isArray(v) ? v.length > 0 : v === true || (typeof v === "string" && v.trim() !== "");
  }
  return true;
}

/**
 * A child element edited inside its parent (spec 4.5 Feature editor, 13.6 dialog): name, the level it is gained at (the
 * parent's grid), its generated parts, rule blocks (each can be gained again at other levels), description and sheet
 * text. Every change is one undoable edit of the parent's draft; Remove takes it off the parent.
 */
export function ChildDialog({ editing, childKey, levelField, onRemove, onClose }: {
  editing: ChildEditing; childKey: string;
  /** The parent's levelled children field (the LEVELS grid), when the child has a level. */
  levelField?: string;
  onRemove: () => void; onClose: () => void;
}) {
  const index = editing.form.children.findIndex((c) => c.key === childKey);
  const child = editing.form.children[index];
  if (!child) return null;
  const kind = editing.kinds.find((k) => k.id === child.kind);
  const problems = childProblems(editing.problems, index);
  const level = levelField ? levelOfChild(editing.form, levelField, childKey) ?? 1 : undefined;
  const set = (f: HomebrewFieldSpec, v: unknown, typing: boolean) => editing.edit((form) => {
    const c = form.children.find((x) => x.key === childKey);
    if (!c) return form;
    const next = f.control === "blocks" ? { ...c, blocks: v as HomebrewForm["blocks"] } : { ...c, fields: { ...c.fields, [f.key]: v } };
    return withChild(form, childKey, next);
  }, typing ? `child:${childKey}:${f.key}` : null);
  const fields = (kind?.fields ?? []).filter((f) => shown(f, child, editing.edition));
  const label = kind?.label ?? "Feature";
  // the child's own children (a variant's feature, a trait's lineage options) are made and edited inside it
  const inner = nestedEditing(editing, childKey) ?? undefined;
  const note = (f: HomebrewFieldSpec) => (otherEdition(f, editing.edition) ? editionNote(f) : undefined);
  return (
    <Dialog title={`${label}: ${nameOf(child, `New ${label.toLowerCase()}`)}`} onClose={onClose} className="hb-child-dialog" wide
      footer={<><Button variant="danger" onClick={onRemove}>Remove from {nameOf(editing.form, "the class")}</Button><Button variant="primary" onClick={onClose}>Done</Button></>}>
      <div className="hb-child-body" data-child={childKey}>
        <div className="mg-grid two">
          {fields.filter((f) => f.tab === "general").map((f, i) => (
            <FieldViewWithLevel key={f.key} first={i === 0} level={level} onLevel={(n) => editing.edit((form) => withChildLevel(form, levelField!, childKey, n))}>
              <FieldView field={f} value={f.control === "blocks" ? child.blocks : child.fields[f.key]} problems={problems} kindId={child.kind} idPrefix={`c-${childKey}-`}
                childEditing={inner} note={note(f)} onChange={(v, typing) => set(f, v, typing)} />
            </FieldViewWithLevel>
          ))}
        </div>
        {(["rules", "description"] as const).map((tab) => (
          <div key={tab} className="hb-section">
            <div className="mg-grid two">
              {fields.filter((f) => f.tab === tab).map((f) => (
                <FieldView key={f.key} field={f} value={f.control === "blocks" ? child.blocks : child.fields[f.key]} problems={problems} kindId={child.kind}
                  idPrefix={`c-${childKey}-`} childEditing={inner} note={note(f)} onChange={(v, typing) => set(f, v, typing)} />
              ))}
            </div>
          </div>
        ))}
        <PreservedList parts={child.preserved} onRemove={(key) => editing.edit((form) => withChild(form, childKey, { ...child, preserved: child.preserved.filter((p) => p.key !== key) }))} />
      </div>
    </Dialog>
  );
}

/** The first general field (the name), then "Level gained" from the parent's grid. */
function FieldViewWithLevel({ first, level, onLevel, children }: { first: boolean; level: number | undefined; onLevel: (n: number) => void; children: ReactNode }) {
  if (!first || level === undefined) return <>{children}</>;
  return (
    <>
      {children}
      <div className="mg-f hb-field" data-field="levelGained">
        <label htmlFor="hb-child-level">Level gained</label>
        <span id="hb-child-level" className="hb-number"><NumericUpDown label="Level gained" value={level} min={1} max={20} onChange={onLevel} /></span>
      </div>
    </>
  );
}
