import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import type { Ctx } from './flows-a.js';

// axe-core in the web e2e run (spec 18.5, board item H-aZY): a check fails on any critical or serious violation.

const require = createRequire(import.meta.url);
const AXE_SOURCE = readFileSync(require.resolve('axe-core/axe.min.js'), 'utf8');

/**
 * Rules left out of the check, each for a recorded reason (never a blanket disable):
 * - `color-contrast`: Borealis keeps Aurora's colours even where they fail WCAG AA contrast (decisions U7 and HC10 Q7,
 *   2026-11-12: strict U7, this rule stays excluded by this documented exception). Only this one rule is left out; every
 *   other contrast-independent rule still fails the run.
 */
export const AXE_EXCLUDED_RULES: readonly string[] = ['color-contrast'];

/** The impacts that fail the run. Moderate and minor findings are audit work (WB-26 section 17), not a gate. */
const FAILING_IMPACTS = ['critical', 'serious'];

export interface AxeFinding { id: string; impact: string; help: string; nodes: string[] }

/** Runs axe on the page (or the `include` subtree) and returns its critical and serious violations. */
export async function axeViolations(ctx: Ctx, include?: string): Promise<AxeFinding[]> {
  await ctx.browser.evaluate((src: string) => {
    const w = window as unknown as { axe?: unknown };
    if (!w.axe) (0, eval)(src);
    return true;
  }, AXE_SOURCE);
  return ctx.browser.evaluate(async ({ inc, off, impacts }: { inc: string | null; off: string[]; impacts: string[] }) => {
    const axe = (window as unknown as { axe: { run: (ctx: unknown, opts: unknown) => Promise<{ violations: { id: string; impact: string | null; help: string; nodes: { target: unknown[] }[] }[] }> } }).axe;
    const context = inc ? document.querySelector(inc) ?? document : document;
    const rules = Object.fromEntries(off.map((id) => [id, { enabled: false }]));
    const res = await axe.run(context, { resultTypes: ['violations'], rules });
    return res.violations
      .filter((v) => impacts.includes(v.impact ?? ''))
      .map((v) => ({ id: v.id, impact: v.impact ?? '', help: v.help, nodes: v.nodes.slice(0, 5).map((n) => JSON.stringify(n.target)) }));
  }, { inc: include ?? null, off: [...AXE_EXCLUDED_RULES], impacts: FAILING_IMPACTS });
}

/** Collects every screen's findings, then fails once with all of them (one run shows every screen's problems). */
export class AxeReport {
  private readonly found: { screen: string; findings: AxeFinding[] }[] = [];
  private screens = 0;

  async check(ctx: Ctx, screen: string, include?: string): Promise<void> {
    this.screens += 1;
    const findings = await axeViolations(ctx, include);
    if (findings.length > 0) this.found.push({ screen, findings });
  }

  /** Throws with every screen's critical and serious violations; returns how many screens were checked. */
  assertClean(): number {
    if (this.found.length === 0) return this.screens;
    const lines = this.found.flatMap(({ screen, findings }) =>
      findings.map((f) => `${screen}: ${f.impact} ${f.id} (${f.help}) at ${f.nodes.join(' ')}`));
    throw new Error(`axe: ${lines.length} critical/serious violation(s) on ${this.found.length} of ${this.screens} screens\n${lines.join('\n')}`);
  }
}
