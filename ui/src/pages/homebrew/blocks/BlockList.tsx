import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import type { HomebrewBlock, HomebrewBlockSpec, HomebrewBuilderProblem, HomebrewForm, HomebrewProblem } from "../../../api/contract";
import { Icon } from "../../../components";
import { setStatus } from "../../../state/status";
import { MgIcon } from "../../manage/mgIcons";
import "../../manage/manage.css";
import "../../manage/manage-fx15.css";
import { useCreatorEnv } from "../env";
import { matchBlockKey } from "../keymap";
import { AddBlockDialog } from "./AddBlockDialog";
import { BlockDialog } from "./BlockDialog";
import { shownSentence } from "./sentence";
import { newBlock } from "./values";

/** Kinds whose blocks can be gained again at other levels (`borealis-authoring::form::repeat`, WB-16). */
export const REPEAT_KINDS: readonly string[] = ["classFeature"];

/** The reserved block field: the levels a block is gained again at. */
export const AGAIN_AT = "againAt";

/** The levels a block is gained again at. */
export const againAt = (b: HomebrewBlock): number[] => (Array.isArray(b.fields[AGAIN_AT]) ? (b.fields[AGAIN_AT] as number[]).filter((n) => typeof n === "number") : []);

/** The window event the creator's Insert / Ctrl+Enter key sends to the RULES list (spec 12.2 `addBlock`). */
export const ADD_BLOCK_EVENT = "hb-add-block";

export interface BlockListProps {
  /** The field label ("Rules", "While on"). */
  label: string;
  blocks: readonly HomebrewBlock[];
  onChange: (blocks: HomebrewBlock[]) => void;
  /** Kinds the list may hold (a toggle's effects); absent = every top-level kind. */
  kinds?: readonly string[];
  /** The draft's problems (paths `blocks.N...`); a nested list gets its own, made relative to it. */
  problems: readonly HomebrewProblem[];
  /** The last form the backend answered with: its sentences replace the fallback ones while the block is unchanged. */
  answer: HomebrewForm | null;
  /** A list inside a block dialog (no global keys). */
  nested?: boolean;
  /** Its blocks can be gained again at other levels (a class feature's, spec 4.5). */
  repeat?: boolean;
}

type Editing = { index: number; block: HomebrewBlock; isNew: boolean; spec: HomebrewBlockSpec };

/** Problems of block `i`, relative to its fields (`blocks.2.fields.number` -> `number`). */
export function blockProblems(problems: readonly HomebrewProblem[], i: number): HomebrewBuilderProblem[] {
  const prefix = `blocks.${i}.fields.`;
  return problems.filter((p) => p.severity !== "info" && p.path?.startsWith(prefix)).map((p) => ({ path: p.path!.slice(prefix.length), message: p.message }));
}

/** Every problem of block `i` (its fields and the block itself). */
const problemsOfBlock = (problems: readonly HomebrewProblem[], i: number): HomebrewProblem[] =>
  problems.filter((p) => p.severity !== "info" && (p.path === `blocks.${i}` || p.path?.startsWith(`blocks.${i}.`)));

/**
 * The rule block list (spec 6, 13.6): the Attacks & Spellcasting list editor. A toolbar with Add (ListAddBelow), Move up,
 * Move down; one card per block with its plain sentence, Edit and Remove. A block is focusable and named by its sentence;
 * Enter edits, Delete removes (undoable), Alt+Up / Alt+Down move it (spec 12.2, 17). A block with problems is marked
 * with an icon and the problem in words.
 */
export function BlockList({ label, blocks, onChange, kinds, problems, answer, nested, repeat = false }: BlockListProps) {
  const env = useCreatorEnv();
  const [sel, setSel] = useState<number | null>(null);
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<Editing | null>(null);
  const list = useRef<HTMLUListElement>(null);
  const specOf = (kind: string) => env.blocks.find((b) => b.kind === kind);

  useEffect(() => {
    if (nested) return undefined;
    const open = () => setAdding(true);
    window.addEventListener(ADD_BLOCK_EVENT, open);
    return () => window.removeEventListener(ADD_BLOCK_EVENT, open);
  }, [nested]);

  const focusRow = (i: number) => setTimeout(() => list.current?.querySelectorAll<HTMLElement>(".hb-block")[i]?.focus(), 0);
  const move = (i: number, by: -1 | 1) => {
    const j = i + by;
    if (j < 0 || j >= blocks.length) return;
    const next = [...blocks];
    [next[i], next[j]] = [next[j]!, next[i]!];
    onChange(next);
    setSel(j);
    focusRow(j);
    setStatus(`Rule moved ${by < 0 ? "up" : "down"}.`);
  };
  const remove = (i: number) => {
    onChange(blocks.filter((_, j) => j !== i));
    setSel(null);
    setStatus("Rule removed. Undo brings it back.");
  };
  const edit = (i: number) => {
    const block = blocks[i];
    const spec = block && specOf(block.kind);
    if (block && spec) setEditing({ index: i, block, isNew: false, spec });
  };
  const onRowKey = (e: KeyboardEvent<HTMLLIElement>, i: number) => {
    if (e.target !== e.currentTarget) return;
    if (e.altKey && !e.ctrlKey && !e.metaKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      e.stopPropagation();
      move(i, e.key === "ArrowUp" ? -1 : 1);
      return;
    }
    const action = matchBlockKey(e);
    if (!action) return;
    e.preventDefault();
    if (action.id === "editBlock") edit(i);
    if (action.id === "deleteBlock") remove(i);
  };
  const done = (b: HomebrewBlock) => {
    if (!editing) return;
    const next = editing.isNew ? [...blocks, b] : blocks.map((x, j) => (j === editing.index ? b : x));
    const at = editing.isNew ? next.length - 1 : editing.index;
    setEditing(null);
    onChange(next);
    setSel(at);
    focusRow(at);
    setStatus(editing.isNew ? "Rule added." : "Rule changed.");
  };

  return (
    <div className="hb-blocks">
      <div className="mg-toolbar" role="toolbar" aria-label={`${label} commands`}>
        <button type="button" className="mg-tool wide hb-add-rule" onClick={() => setAdding(true)} title="Add a rule (Insert)"><MgIcon name="addAttack" />Add rule</button>
        <button type="button" className="mg-tool" aria-label="Move selected rule up" title="Move selected rule up (Alt+Up)" disabled={sel === null || sel <= 0} onClick={() => sel !== null && move(sel, -1)}><MgIcon name="moveUp" /></button>
        <button type="button" className="mg-tool" aria-label="Move selected rule down" title="Move selected rule down (Alt+Down)" disabled={sel === null || sel >= blocks.length - 1} onClick={() => sel !== null && move(sel, 1)}><MgIcon name="moveDown" /></button>
      </div>
      {blocks.length === 0 && <p className="hb-field-help">No rules yet. Add one to give the element something to do.</p>}
      {blocks.length > 0 && (
        <ul className="atk-list hb-block-list" aria-label={label} ref={list}>
          {blocks.map((b, i) => {
            const spec = specOf(b.kind);
            const again = againAt(b);
            const sentence = again.length > 0 ? `${shownSentence(b, answer)} Again at level${again.length > 1 ? "s" : ""} ${again.join(", ")}.` : shownSentence(b, answer);
            const bad = problemsOfBlock(problems, i);
            const kept = b.preserved?.length ?? 0;
            return (
              <li key={b.key} className={`atk hb-block ${bad.length ? "hb-block-bad" : ""}`} tabIndex={0} aria-current={sel === i} aria-label={sentence}
                data-block={i} onClick={() => setSel(i)} onFocus={() => setSel(i)} onDoubleClick={() => edit(i)} onKeyDown={(e) => onRowKey(e, i)}>
                <div className="info">
                  <h3>{bad.length > 0 && <Icon name="x" />}{spec?.label ?? "Rule kept as is"}</h3>
                  <p>{sentence}</p>
                  {(kept > 0 || !spec) && <span className="ap-tags"><span className="ap-tag">{spec ? `${kept} kept ${kept === 1 ? "part" : "parts"}` : "Kept as is"}</span></span>}
                  {bad.map((p, j) => <p key={j} className="hb-block-msg">{p.message}</p>)}
                </div>
                <span className="btns">
                  <button type="button" aria-label={`Edit: ${sentence}`} title="Edit (Enter)" disabled={!spec} onClick={(e) => { e.stopPropagation(); edit(i); }}><MgIcon name="edit" /></button>
                  <button type="button" aria-label={`Remove: ${sentence}`} title="Remove (Delete)" onClick={(e) => { e.stopPropagation(); remove(i); }}><MgIcon name="cardMinus" /></button>
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {adding && (
        <AddBlockDialog kinds={kinds} onClose={() => setAdding(false)}
          onPick={(spec) => { setAdding(false); setEditing({ index: blocks.length, block: newBlock(spec), isNew: true, spec }); }} />
      )}
      {editing && (
        <BlockDialog spec={editing.spec} block={editing.block} isNew={editing.isNew} problems={editing.isNew ? [] : blockProblems(problems, editing.index)} repeat={repeat}
          onOk={done} onCancel={() => setEditing(null)} />
      )}
    </div>
  );
}
