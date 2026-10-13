import type { SemanticNode } from 'e2e/engine';

/** The subset of agent-device's snapshot node this projection reads. */
export interface RawNode {
  readonly index?: number;
  readonly ref?: string;
  readonly type?: string;
  readonly role?: string;
  readonly subrole?: string;
  readonly label?: string;
  readonly value?: string;
  readonly identifier?: string;
  readonly placeholder?: string;
  readonly rect?: { readonly x: number; readonly y: number; readonly width: number; readonly height: number };
  readonly depth?: number;
  readonly parentIndex?: number;
  readonly enabled?: boolean;
  readonly selected?: boolean;
  readonly focused?: boolean;
  readonly checked?: boolean;
  readonly heading?: boolean;
  readonly editable?: boolean;
  readonly password?: boolean;
  readonly hintShowing?: boolean;
  readonly visibleToUser?: boolean;
}

/** AX role spellings (with or without the `AX` prefix, any case or separators) to contract roles. */
const ROLE_MAP: Record<string, string> = {
  button: 'button', link: 'link', statictext: 'text', text: 'text', textfield: 'textbox', textarea: 'textbox',
  searchfield: 'searchbox', securetextfield: 'textbox', combobox: 'combobox', popupbutton: 'combobox', checkbox: 'checkbox',
  radiobutton: 'radio', switch: 'switch', slider: 'slider', image: 'img', group: 'group', list: 'list', listitem: 'listitem',
  row: 'row', cell: 'cell', table: 'table', heading: 'heading', menu: 'menu', menuitem: 'menuitem', menubar: 'menubar',
  menubaritem: 'menuitem', tabgroup: 'tablist', tab: 'tab', window: 'window', sheet: 'dialog', dialog: 'dialog',
  webarea: 'document', scrollarea: 'group', toolbar: 'toolbar', progressindicator: 'progressbar', splitgroup: 'group',
};

const normalizeKind = (raw: string): string => raw.replace(/^AX/, '').replace(/[^A-Za-z0-9]/g, '').toLowerCase();
const nonEmpty = (v: string | undefined): string | undefined => (v === undefined || v === '' ? undefined : v);

function roleOf(node: RawNode): string | undefined {
  const sub = normalizeKind(node.subrole ?? '');
  if (sub.includes('dialog') && !sub.includes('standard')) return 'dialog';
  if (node.heading === true) return 'heading';
  const mapped = ROLE_MAP[normalizeKind(node.type ?? node.role ?? '')];
  const role = mapped ?? (normalizeKind(node.type ?? node.role ?? '') || undefined);
  if (node.editable === true && role !== 'textbox' && role !== 'searchbox' && role !== 'combobox') return 'textbox';
  return role;
}

/** Parent position per node: `parentIndex` when it names an earlier node, else reconstructed from `depth`. */
function parentPositions(raw: readonly RawNode[]): (number | undefined)[] {
  const byIndex = new Map<number, number>();
  raw.forEach((n, i) => { if (typeof n.index === 'number') byIndex.set(n.index, i); });
  const lastAtDepth: number[] = [];
  return raw.map((n, i) => {
    let parent: number | undefined;
    if (typeof n.parentIndex === 'number' && n.parentIndex >= 0) {
      const found = byIndex.get(n.parentIndex);
      if (found !== undefined && found < i) parent = found;
    }
    if (parent === undefined && typeof n.depth === 'number' && n.depth > 0) parent = lastAtDepth[n.depth - 1];
    if (typeof n.depth === 'number') { lastAtDepth[n.depth] = i; lastAtDepth.length = n.depth + 1; }
    return parent;
  });
}

export interface Projection {
  readonly roots: SemanticNode[];
  /** node id to the agent-device ref (`@e5`) that acts on it. */
  readonly refs: Map<string, string>;
  /** node id to the centre of its rect, in screen points (where a real mouse click lands). */
  readonly centers: Map<string, { x: number; y: number }>;
  readonly viewport: { width: number; height: number };
  /** Screen position of the window's top-left (raw rects are screen coordinates). */
  readonly origin: { x: number; y: number };
}

/** Projects one snapshot to the contract tree. `mintId` is called once per node in document order. */
export function projectSnapshot(raw: readonly RawNode[], mintId: () => string): Projection {
  const parents = parentPositions(raw);
  const children = new Map<number, number[]>();
  const roots: number[] = [];
  parents.forEach((p, i) => {
    if (p === undefined) roots.push(i);
    else children.set(p, [...(children.get(p) ?? []), i]);
  });
  const refs = new Map<string, string>();
  const centers = new Map<string, { x: number; y: number }>();
  const build = (i: number): SemanticNode => {
    const n = raw[i]!;
    const id = mintId();
    if (nonEmpty(n.ref) !== undefined) refs.set(id, n.ref!.startsWith('@') ? n.ref! : `@${n.ref}`);
    if (n.rect !== undefined) centers.set(id, { x: Math.round(n.rect.x + n.rect.width / 2), y: Math.round(n.rect.y + n.rect.height / 2) });
    const role = roleOf(n);
    const secure = n.password === true || normalizeKind(n.type ?? '') === 'securetextfield';
    const label = nonEmpty(n.label);
    const value = secure || n.hintShowing === true ? undefined : nonEmpty(n.value);
    const states = {
      ...(n.enabled === false ? { disabled: true } : {}),
      ...(n.selected === true ? { selected: true } : {}),
      ...(n.focused === true ? { focused: true } : {}),
      ...(n.visibleToUser === false ? { hidden: true } : {}),
      ...(secure ? { secure: true } : {}),
      ...(n.checked === undefined ? {} : { checked: n.checked }),
    };
    const kids = (children.get(i) ?? []).map(build);
    return {
      ref: { id, revision: '' },
      ...(role === undefined ? {} : { role }),
      ...(label === undefined ? {} : { name: label, text: label }),
      ...(value === undefined || value === label ? {} : { value }),
      ...(secure ? { inputPurpose: 'password' as const } : {}),
      ...(Object.keys(states).length === 0 ? {} : { states }),
      ...(nonEmpty(n.placeholder) === undefined ? {} : { attributes: { placeholder: n.placeholder! } }),
      ...(nonEmpty(n.identifier) === undefined ? {} : { testId: n.identifier! }),
      ...(n.rect === undefined ? {} : { rect: { ...n.rect } }),
      ...(kids.length === 0 ? {} : { children: kids }),
    } as SemanticNode;
  };
  const built = roots.map(build);
  const biggest = raw.reduce((best, n) => (n.rect !== undefined && n.rect.width * n.rect.height > best.width * best.height ? n.rect : best), { width: 0, height: 0 } as { width: number; height: number });
  const win = raw.find((n) => normalizeKind(n.type ?? '') === 'window' && n.rect !== undefined)?.rect;
  const origin = { x: win?.x ?? 0, y: win?.y ?? 0 };
  return { roots: built, refs, centers, origin, viewport: biggest.width > 0 ? { width: biggest.width, height: biggest.height } : { width: 1355, height: 847 } };
}
