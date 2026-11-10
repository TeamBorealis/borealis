import type { HomebrewProblem } from "../../api/contract";
import { Expander, Icon } from "../../components";
import { countProblems } from "./problems";

const ICON = { error: "x", warning: "alert", info: "info" } as const;

/**
 * PROBLEMS (n) at the top of the editor (spec 8.3, borrowed from the Build expander, 13.12): one row per problem, as a plain
 * sentence; a row jumps to its field. Opening it runs the engine checks (8.1 step 6).
 */
export function ProblemsPanel({ problems, checking, onJump, onOpen }: {
  problems: readonly HomebrewProblem[]; checking: boolean; onJump: (p: HomebrewProblem) => void; onOpen: () => void;
}) {
  const c = countProblems(problems);
  const n = c.errors + c.warnings;
  return (
    <div className="hb-problems" data-errors={c.errors > 0 || undefined}>
      <Expander title={`Problems (${n})`} defaultOpen={c.errors > 0} onToggle={(open) => { if (open) onOpen(); }}
        headerExtra={checking ? <span className="hb-checking">Checking...</span> : undefined}>
        {problems.length === 0 ? <p className="hb-problems-none">No problems found.</p> : (
          <ul className="hb-problem-list" aria-label="Problems">
            {problems.map((p, i) => (
              <li key={`${p.code}-${p.path ?? ""}-${i}`}>
                <button type="button" className={`hb-problem hb-${p.severity}`} onClick={() => onJump(p)}>
                  <Icon name={ICON[p.severity]} /><span>{p.message}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Expander>
    </div>
  );
}
