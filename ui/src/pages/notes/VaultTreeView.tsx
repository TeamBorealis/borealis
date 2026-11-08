import { mdiChevronDown, mdiChevronRight, mdiCloudDownloadOutline, mdiFileDocumentOutline, mdiFileOutline, mdiFolderOutline, mdiImageOutline, mdiVectorSquare } from "@mdi/js";
import { useMemo } from "react";
import type { VaultEntry } from "../../api/contract";
import { Svg, displayName } from "./noteNames";

/** A folder or file and, for folders, what is in it (in the backend's order: folders first, then files, by name). */
export interface TreeNode {
  entry: VaultEntry;
  children: TreeNode[];
}

/** Nest the backend's depth-first listing. */
export function buildTree(entries: readonly VaultEntry[]): TreeNode[] {
  const roots: TreeNode[] = [];
  const folders = new Map<string, TreeNode>();
  for (const entry of entries) {
    const node: TreeNode = { entry, children: [] };
    const cut = entry.path.lastIndexOf("/");
    const parent = cut < 0 ? null : folders.get(entry.path.slice(0, cut));
    (parent ? parent.children : roots).push(node);
    if (entry.kind === "folder") folders.set(entry.path, node);
  }
  return roots;
}

/** The folders that hold `path` (to show an opened note in the tree). */
export function ancestors(path: string): string[] {
  const parts = path.split("/");
  return parts.slice(0, -1).map((_, i) => parts.slice(0, i + 1).join("/"));
}

const ICONS: Record<VaultEntry["kind"], string> = {
  folder: mdiFolderOutline, note: mdiFileDocumentOutline, image: mdiImageOutline, canvas: mdiVectorSquare, file: mdiFileOutline,
};

interface RowsProps {
  nodes: TreeNode[];
  depth: number;
  expanded: ReadonlySet<string>;
  selected: string | null;
  onToggle: (folder: string) => void;
  onOpen: (entry: VaultEntry) => void;
}

function Rows({ nodes, depth, expanded, selected, onToggle, onOpen }: RowsProps) {
  return (
    <>
      {nodes.map(({ entry, children }) => {
        const folder = entry.kind === "folder";
        const open = folder && expanded.has(entry.path);
        return (
          <li key={entry.path} role="treeitem" aria-level={depth + 1} aria-expanded={folder ? open : undefined} aria-selected={entry.path === selected}>
            <button type="button" className={`nt-row${entry.path === selected ? " sel" : ""}`} style={{ paddingLeft: 6 + depth * 14 }}
              title={entry.path} data-kind={entry.kind} onClick={() => (folder ? onToggle(entry.path) : onOpen(entry))}>
              <span className="nt-caret">{folder && <Svg d={open ? mdiChevronDown : mdiChevronRight} />}</span>
              <Svg d={ICONS[entry.kind]} />
              <span className="nt-name">{displayName(entry.name, entry.kind)}</span>
              {entry.cloud && <span className="nt-cloud" title="Not downloaded from iCloud yet"><Svg d={mdiCloudDownloadOutline} /><span className="sr-only">, not downloaded yet</span></span>}
            </button>
            {open && children.length > 0 && (
              <ul role="group"><Rows nodes={children} depth={depth + 1} expanded={expanded} selected={selected} onToggle={onToggle} onOpen={onOpen} /></ul>
            )}
          </li>
        );
      })}
    </>
  );
}

/** The vault's folder tree: notes, images and every other file, folders open and close. */
export function VaultTreeView({ entries, expanded, selected, onToggle, onOpen }: Omit<RowsProps, "nodes" | "depth"> & { entries: readonly VaultEntry[] }) {
  const nodes = useMemo(() => buildTree(entries), [entries]);
  if (nodes.length === 0) return <p className="nt-empty">This vault has no notes yet.</p>;
  return (
    <ul className="nt-tree" role="tree" aria-label="Vault files">
      <Rows nodes={nodes} depth={0} expanded={expanded} selected={selected} onToggle={onToggle} onOpen={onOpen} />
    </ul>
  );
}
