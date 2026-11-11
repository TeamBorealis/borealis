import { useEffect, useMemo, useRef, useState } from "react";
import type { HomebrewKindDescriptor, HomebrewProblem, HomebrewTrialChange, Ruleset } from "../../api/contract";
import { Button, Expander, Flyout } from "../../components";
import { useApi } from "../../state/ApiContext";
import { ElementDetailView } from "../builder/DescriptionPane";
import { SlotList } from "../builder/SlotList";
import { ProblemsPanel } from "./ProblemsPanel";
import type { DraftController } from "./useDraft";
import { trialSlotOf } from "./tryit/placement";
import { TrialLadder } from "./tryit/TrialLadder";
import { TrialScope } from "./tryit/trialScope";
import { TrialSetup } from "./tryit/TrialSetup";
import { TrialSummary, changedSet } from "./tryit/TrialSummary";
import { useTrial } from "./tryit/useTrial";
import "./tryit/tryit.css";

const NO_CHOICES = "The test character has nothing to choose at this level.";
const NOT_KEPT = "Your latest changes could not be kept, so Try it uses the last version that was.";

/** The level on screen: the one asked for until an answer at that level arrives (stepping stays smooth). */
function useShownLevel(viewLevel: number | null): [number, (level: number) => void] {
  const [asked, setAsked] = useState<number | null>(null);
  useEffect(() => { if (asked !== null && asked === viewLevel) setAsked(null); }, [asked, viewLevel]);
  return [asked ?? viewLevel ?? 1, setAsked];
}

export interface TryItProps {
  kind: HomebrewKindDescriptor;
  draft: DraftController;
  onClose: () => void;
  /** A problem row: close Try it and go to the field it names. */
  onJump: (problem: HomebrewProblem) => void;
}

/**
 * Try it (spec 9, 13.9): the Level up flyout's 400 px right panel (phones: a full-height sheet, spec 16) with a throwaway
 * test character that has the draft. SETUP (the draft in its slot, edition, level, ability scores), CHOICES (every open
 * choice as a Build picker), SUMMARY (the sheet's numbers, values the draft changed marked) and the LEVEL LADDER (1-20).
 * The open character is never touched; the test character is dropped on close.
 */
export function TryIt({ kind, draft, onClose, onJump }: TryItProps) {
  const api = useApi();
  const [ready, setReady] = useState(false);
  const [notKept, setNotKept] = useState(false);
  const [generation, setGeneration] = useState(0);
  const [asSecondClass, setAsSecondClass] = useState(false);
  const flushed = useRef(false);
  // Try it reads the draft as the backend has it: send what is still waiting first.
  useEffect(() => {
    if (flushed.current) return;
    flushed.current = true;
    void draft.flush().then((ok) => { setNotKept(!ok); setReady(true); });
  }, [draft]);
  const draftId = ready ? draft.view?.draftId ?? null : null;
  const { session, state } = useTrial(api, draftId, generation);
  const view = state.view;
  const [level, askLevel] = useShownLevel(view?.snapshot.level.total ?? null);
  const changed = useMemo(() => changedSet(view?.changed ?? []), [view?.changed]);
  const slot = trialSlotOf(kind);
  const name = typeof draft.form?.fields.name === "string" && draft.form.fields.name.trim() ? draft.form.fields.name.trim() : `New ${kind.label.toLowerCase()}`;

  const send = (change: HomebrewTrialChange) => { void session?.change(change).catch(() => undefined); };
  const setLevel = (n: number) => { askLevel(n); send({ level: n }); };
  const setEdition = (edition: Ruleset) => send({ setup: { edition } });
  const setSecondClass = (on: boolean) => { setAsSecondClass(on); send({ setup: { asSecondClass: on } }); };
  const startOver = () => { setAsSecondClass(false); setGeneration((g) => g + 1); };
  const jump = (p: HomebrewProblem) => { onClose(); onJump(p); };
  const open = view ? view.snapshot.slots.filter((s) => s.required && !s.chosen && !s.locked).length : 0;

  return (
    <Flyout title="Try it" onClose={onClose}
      footer={<div className="v4-foot"><Button onClick={startOver} disabled={!view}>Start over</Button><Button onClick={onClose}>Close</Button></div>}>
      <div className="v4-new hb-try" aria-busy={state.busy || !view}>
        <p className="sub">A test character with {name}. It is never saved and never listed with your characters.</p>
        {notKept && <p className="sub hb-try-warn" role="status">{NOT_KEPT}</p>}
        {state.notice && <p className="sub" role="status">{state.notice}</p>}
        {state.error && <p className="hb-error-line" role="alert">{state.error}</p>}
        {!view || !session ? (!state.error && <p className="sub" role="status">Making the test character...</p>) : (
          <>
            <p className="lvl-status hb-try-status">
              <span>Your test character is level {view.snapshot.level.total}</span>
              {state.busy && <span className="hb-try-busy" role="status">Updating...</span>}
            </p>
            <ProblemsPanel problems={view.problems} checking={false} onJump={jump} onOpen={() => undefined} />
            {slot === "creature" && draft.view?.preview && (
              <Expander flyout title="Stat block">
                <div className="hb-try-statblock"><ElementDetailView d={draft.view.preview} /></div>
              </Expander>
            )}
            <Expander flyout title="Setup">
              <TrialSetup kind={kind} draftName={name} slot={slot} view={view} level={level} asSecondClass={asSecondClass}
                onLevel={setLevel} onEdition={setEdition} onScores={(abilityScores) => send({ setup: { abilityScores } })} onSecondClass={setSecondClass} />
            </Expander>
            <Expander flyout title={`Choices (${open})`}>
              <TrialScope session={session} view={view} revision={state.revision}>
                <SlotList slots={view.snapshot.slots} emptyText={NO_CHOICES} />
              </TrialScope>
            </Expander>
            <Expander flyout title="Summary">
              {view.changed.length > 0 && <p className="sub hint"><span className="hb-try-mark" aria-hidden="true">*</span> Changed by this {kind.label.toLowerCase()}.</p>}
              <TrialSummary snapshot={view.snapshot} changed={changed} />
            </Expander>
            <Expander flyout title="Level ladder">
              <TrialLadder ladder={view.ladder} level={level} onLevel={setLevel} />
            </Expander>
          </>
        )}
      </div>
    </Flyout>
  );
}
