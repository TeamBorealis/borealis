import type { ReactNode } from "react";
import type { HomebrewAbility, HomebrewBuilderProblem, HomebrewExprValue, HomebrewFormulaDialect, HomebrewFormulaNode, HomebrewFormulaOp } from "../../../api/contract";
import { ComboBox, Icon, NumericUpDown } from "../../../components";
import { classChoices, className, useCreatorEnv } from "../env";
import { KeptChip } from "./ExprTree";
import {
  ABILITY_CHOICES, DIALECT_TERMS, FORMULA_PRESETS, type FormulaTerm, formulaWords, newTerm, OP_WORDS, problemsAt, SUM_ONLY, TERM_WORDS, withModel,
} from "./words";

const NOT_SET = "";
/** Number range the builder writes (hc-wb02: -999..999, 0..999 in save DCs). */
const NUMBER_MAX = 999;

/** Terms whose parts nest below the term (indented), not beside it. */
const NESTED: ReadonlySet<FormulaTerm> = new Set(["binary", "max", "min", "floor", "ceilDiv"]);

const join = (path: string, more: string): string => (path ? `${path}.${more}` : more);

interface NodeProps {
  node: HomebrewFormulaNode;
  onNode: (n: HomebrewFormulaNode) => void;
  dialect: HomebrewFormulaDialect;
  label: string;
  path: string;
  problems: readonly HomebrewBuilderProblem[];
  depth: number;
}

function Problems({ list }: { list: readonly HomebrewBuilderProblem[] }) {
  return list.length ? <div className="hb-row-msg" role="note">{list.map((p, i) => <p key={i}><Icon name="x" />{p.message}</p>)}</div> : null;
}

/** One formula part: its term combo and the term's own parts; combined parts nest below, indented. */
function FormulaNodeEditor({ node, onNode, dialect, label, path, problems, depth }: NodeProps) {
  const env = useCreatorEnv();
  const classes = classChoices(env);
  const terms = DIALECT_TERMS[dialect].filter((t) => t !== "binary" || depth < 4);
  const child = (n: HomebrewFormulaNode, set: (x: HomebrewFormulaNode) => void, part: string, l: string) => (
    <FormulaNodeEditor node={n} onNode={set} dialect={dialect} label={`${label}, ${l}`} path={join(path, part)} problems={problems} depth={depth + 1} />
  );
  const classCombo = (value: string, set: (c: string) => void) => (
    <ComboBox aria-label={`${label}: class`} value={value} onChange={set}
      options={[{ value: "", label: "Choose a class" }, ...(value && !classes.some((c) => c.value === value) ? [{ value, label: className(env, value) }] : []), ...classes]} />
  );
  const own = problems.filter((p) => p.path === path);
  const head = (
    <ComboBox aria-label={`${label}: part`} value={node.term} onChange={(t) => onNode(newTerm(t as FormulaTerm, dialect))}
      options={terms.map((t) => ({ value: t, label: TERM_WORDS[t] }))} />
  );
  let body: ReactNode = null;
  switch (node.term) {
    case "number":
      body = <NumericUpDown label={`${label}: number`} value={node.value} min={SUM_ONLY.has(dialect) ? 0 : -NUMBER_MAX} max={NUMBER_MAX} onChange={(value) => onNode({ ...node, value })} compact />;
      break;
    case "ability":
      body = <ComboBox aria-label={`${label}: ability`} value={node.ability} options={ABILITY_CHOICES} onChange={(a) => onNode({ ...node, ability: a as HomebrewAbility })} />;
      break;
    case "classLevel": case "classSaveDc":
      body = classCombo(node.class, (c) => onNode({ ...node, class: c }));
      break;
    case "binary": {
      const ops = (SUM_ONLY.has(dialect) ? ["add"] : ["add", "sub", "mul", "div"]) as HomebrewFormulaOp[];
      body = (
        <div className="hb-formula-sub">
          {child(node.left, (left) => onNode({ ...node, left }), "left", "first part")}
          {ops.length > 1
            ? <ComboBox aria-label={`${label}: combined by`} value={node.op} options={ops.map((o) => ({ value: o, label: OP_WORDS[o] }))} onChange={(op) => onNode({ ...node, op: op as HomebrewFormulaOp })} />
            : <span className="hb-formula-op">plus</span>}
          {child(node.right, (right) => onNode({ ...node, right }), "right", "second part")}
        </div>
      );
      break;
    }
    case "max": case "min":
      body = (
        <div className="hb-formula-sub">
          {node.args.map((a, i) => (
            <div key={i} className="hb-formula-arg">
              {child(a, (x) => onNode({ ...node, args: node.args.map((y, j) => (j === i ? x : y)) }), `args.${i}`, `value ${i + 1}`)}
              {node.args.length > 1 && <button type="button" className="hb-chip-x" aria-label={`Remove value ${i + 1}`} onClick={() => onNode({ ...node, args: node.args.filter((_, j) => j !== i) })}><Icon name="x" /></button>}
            </div>
          ))}
          <button type="button" className="hb-pick-btn" onClick={() => onNode({ ...node, args: [...node.args, { term: "number", value: 1 }] })}><Icon name="plus" />Add value</button>
        </div>
      );
      break;
    case "floor":
      body = <div className="hb-formula-sub">{child(node.arg, (arg) => onNode({ ...node, arg }), "arg", "value")}</div>;
      break;
    case "ceilDiv":
      body = (
        <div className="hb-formula-sub">
          {child(node.num, (num) => onNode({ ...node, num }), "num", "value")}
          <span className="hb-formula-op">divided by</span>
          {child(node.den, (den) => onNode({ ...node, den }), "den", "divisor")}
        </div>
      );
      break;
    default:
      break;
  }
  return (
    <div className={`hb-formula-node ${own.length ? "hb-row-bad" : ""}`} data-path={path}>
      <span className="hb-row-controls">{head}{!NESTED.has(node.term) && body}</span>
      {NESTED.has(node.term) && body}
      <Problems list={own} />
    </div>
  );
}

export interface FormulaBuilderProps {
  label: string;
  value: HomebrewExprValue<HomebrewFormulaNode> | null;
  onChange: (value: HomebrewExprValue<HomebrewFormulaNode> | null) => void;
  dialect: HomebrewFormulaDialect;
  problems: readonly HomebrewBuilderProblem[];
  required?: boolean;
}

/**
 * The formula builder (spec 6.3): parts picked from lists (numbers, proficiency bonus, ability modifiers, class levels,
 * the highest/lowest of, rounding), offering only the terms the field's engine reader reads (`dialect`, hc-wb02), one-tap
 * presets, and the formula read back in words. The text form is never shown.
 */
export function FormulaBuilder({ label, value, onChange, dialect, problems, required }: FormulaBuilderProps) {
  const env = useCreatorEnv();
  if (value?.value.shape === "kept") {
    return <KeptChip text={value.value.text} reason={value.value.reason} what="formula" onRemove={() => onChange(null)} />;
  }
  const node = value?.value.shape === "model" ? value.value.model : null;
  const set = (n: HomebrewFormulaNode) => onChange(withModel(value, n));
  const presets = FORMULA_PRESETS.filter((p) => p.dialects.includes(dialect));
  const firstClass = classChoices(env)[0]?.value ?? "";
  const stray = problemsAt(problems, "").filter((p) => p.path === "");
  return (
    <div className="hb-formula">
      {node ? (
        <div className="hb-formula-root">
          <FormulaNodeEditor node={node} onNode={set} dialect={dialect} label={label} path="" problems={problems} depth={0} />
          {!required && <button type="button" className="hb-chip-x" aria-label={`Clear ${label}`} onClick={() => onChange(null)}><Icon name="x" /></button>}
        </div>
      ) : (
        <ComboBox aria-label={`${label}: part`} value={NOT_SET} onChange={(t) => { if (t) set(newTerm(t as FormulaTerm, dialect)); }}
          options={[{ value: NOT_SET, label: "Not set" }, ...DIALECT_TERMS[dialect].map((t) => ({ value: t, label: TERM_WORDS[t] }))]} />
      )}
      {presets.length > 0 && (
        <div className="hb-presets" role="group" aria-label={`${label}: common shapes`}>
          {presets.map((p) => <button key={p.label} type="button" className="hb-chip" onClick={() => set(p.build(firstClass))}>{p.label}</button>)}
        </div>
      )}
      {node && <p className="hb-formula-words">= {formulaWords(node, (c) => className(env, c))}</p>}
      <Problems list={stray} />
    </div>
  );
}
