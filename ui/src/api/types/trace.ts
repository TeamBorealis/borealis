/** Stat provenance: how a statistic got its value. Mirrors the engine `StatTrace`. */

export type TraceStepKind = "seed" | "derived" | "rule";

export interface TraceStep {
  group: string;
  kind: TraceStepKind;
  label: string;
  elementId?: string;
  elementName?: string;
  ruleKind: string;
  /** Value contributed when `included`. */
  value: number;
  /** The value as written in the rule (number or referenced statistic). */
  rawValue?: string;
  bonus?: string;
  /** Conditions on the rule: level, requirements, equipped. */
  condition?: string;
  included: boolean;
  /** Why the step was excluded, or a note on an included one. */
  reason?: string;
  /** Steps of a referenced statistic; they explain `value` but are not added to the parent's sum. */
  inputs: TraceStep[];
}

export interface TraceDiagnostic {
  code: string;
  severity: "info" | "warning" | "error";
  message: string;
  element?: string;
}

/** Invariant: `value` is the sum of the included top-level step values. */
export interface StatTrace {
  /** Lower-case statistic name such as `ac` or `strength:score`. */
  stat: string;
  base: number;
  steps: TraceStep[];
  value: number;
  diagnostics: TraceDiagnostic[];
}
