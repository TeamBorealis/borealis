import { useEffect, useRef } from "react";

/**
 * The creator's keyboard map (homebrew creator spec 12.2). Scoped to the creator routes: the handler listens in the capture
 * phase on `window`, so it runs before the shell's own shortcuts (`AppShell` `useShortcuts`) and stops the keys it uses
 * from reaching them; Ctrl+S saves the element here and the character everywhere else. Ctrl+N stays "new character".
 * Ctrl and Cmd are the same key here (Cmd on macOS), as in the shell.
 */
export type CreatorActionId =
  | "save" | "undo" | "redo" | "newElement" | "duplicate" | "focusFilter" | "tryIt" | "tab" | "rename"
  | "addBlock" | "editBlock" | "deleteBlock" | "moveUp" | "moveDown";

export interface CreatorAction {
  id: CreatorActionId;
  /** The sub-tab (0-based) for `tab`. */
  index?: number;
}

/** Where a key works: anywhere on the creator route, or only while a rule block (or list row) has focus. */
export type KeyScope = "creator" | "block";

export interface KeyBinding {
  keys: string;
  action: CreatorActionId;
  label: string;
  scope: KeyScope;
}

/** Spec 12.2, in its order. Every action also has a button or a menu item. */
export const CREATOR_KEYMAP: readonly KeyBinding[] = [
  { keys: "Ctrl+S", action: "save", label: "Save the element (not the character)", scope: "creator" },
  { keys: "Ctrl+Z", action: "undo", label: "Undo", scope: "creator" },
  { keys: "Ctrl+Shift+Z", action: "redo", label: "Redo", scope: "creator" },
  { keys: "Ctrl+Shift+N", action: "newElement", label: "New element of the selected kind", scope: "creator" },
  { keys: "Ctrl+D", action: "duplicate", label: "Duplicate the element", scope: "creator" },
  { keys: "Ctrl+F", action: "focusFilter", label: "Filter the element list", scope: "creator" },
  { keys: "Ctrl+T", action: "tryIt", label: "Try it", scope: "creator" },
  { keys: "Ctrl+1..9", action: "tab", label: "Editor sub-tabs", scope: "creator" },
  { keys: "Insert, Ctrl+Enter", action: "addBlock", label: "Add a rule block", scope: "creator" },
  { keys: "Enter", action: "editBlock", label: "Edit the focused block", scope: "block" },
  { keys: "Delete", action: "deleteBlock", label: "Delete the focused block (undoable)", scope: "block" },
  { keys: "Alt+Up", action: "moveUp", label: "Move the focused block or row up", scope: "block" },
  { keys: "Alt+Down", action: "moveDown", label: "Move the focused block or row down", scope: "block" },
  { keys: "F2", action: "rename", label: "Rename", scope: "creator" },
];

/** The parts of a key event the map reads. */
export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

const CTRL_KEYS: Readonly<Record<string, CreatorActionId>> = { s: "save", d: "duplicate", f: "focusFilter", t: "tryIt" };

function withModifier(k: string, shift: boolean): CreatorAction | null {
  if (k === "z") return { id: shift ? "redo" : "undo" };
  if (k === "n") return shift ? { id: "newElement" } : null; // Ctrl+N stays "new character"
  if (k === "enter") return shift ? null : { id: "addBlock" };
  if (/^[1-9]$/.test(k)) return shift ? null : { id: "tab", index: Number(k) - 1 };
  const id = CTRL_KEYS[k];
  return id && !shift ? { id } : null;
}

/** The creator action a key press asks for, or null. Block-scope actions are returned too; only a focused block runs them. */
export function matchCreatorKey(e: KeyLike): CreatorAction | null {
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key.toLowerCase();
  if (mod) return e.altKey ? null : withModifier(k, e.shiftKey);
  if (e.altKey) return k === "arrowup" ? { id: "moveUp" } : k === "arrowdown" ? { id: "moveDown" } : null;
  if (e.shiftKey) return null;
  if (k === "f2") return { id: "rename" };
  if (k === "insert") return { id: "addBlock" };
  return null;
}

/** Block-scope keys that need no modifier (Enter, Delete): the block list (WB-10) handles them on the focused block. */
export function matchBlockKey(e: KeyLike): CreatorAction | null {
  if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return null;
  if (e.key === "Enter") return { id: "editBlock" };
  if (e.key === "Delete") return { id: "deleteBlock" };
  return null;
}

export type CreatorHandlers = Partial<Record<CreatorActionId, (action: CreatorAction) => void>>;

/** True while a modal dialog or flyout is open: its own keys (Escape, typing) win and the creator keys wait. */
const modalOpen = (): boolean => document.querySelector('[aria-modal="true"]') !== null;

/**
 * Runs the creator's global keys while mounted. A key with a handler is taken: its default (the browser's own undo, find,
 * new tab) is prevented and the shell never sees it. "save" is always taken on the creator route, so Ctrl+S never saves
 * the open character from here, even with no element open.
 */
export function useCreatorKeys(handlers: CreatorHandlers): void {
  const ref = useRef(handlers);
  ref.current = handlers;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.isComposing || modalOpen()) return;
      const action = matchCreatorKey(e);
      if (!action) return;
      const run = ref.current[action.id];
      if (!run && action.id !== "save") return;
      e.preventDefault();
      e.stopPropagation();
      run?.(action);
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, []);
}
