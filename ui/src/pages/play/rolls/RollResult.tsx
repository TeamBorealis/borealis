import { useEffect, useState } from "react";
import { Button, EmptyState, IconButton, Modal } from "../../../components";
import { OverlayPortal } from "../../../components/controls/OverlayHost";
import type { Shown } from "./RollContext";
import { type RollEntry, clearRolls, useRolls } from "./rollStore";

/** How long a result stays up; the history keeps it afterwards. */
export const RESULT_MS = 12000;

const time = (at: number) => new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });

/**
 * The newest roll, at the bottom of the screen above the tab bar on phones, in a polite live region so screen readers
 * hear "Stealth check 19, d20 [14, 7] +5 = 19, advantage". An attack roll offers its damage roll (doubled on a crit).
 */
export function RollResultCard({ shown, onDismiss }: { shown: Shown | null; onDismiss: () => void }) {
  const [history, setHistory] = useState(false);
  const id = shown?.entry.id;
  useEffect(() => {
    if (id === undefined) return undefined;
    const t = setTimeout(onDismiss, RESULT_MS);
    return () => clearTimeout(t);
  }, [id, onDismiss]);
  const e = shown?.entry;
  return (
    <OverlayPortal>
      <div className="pr-results">
        {/* what a screen reader hears: always in the page, so each new roll is announced (the card's buttons are not) */}
        <p className="sr-only" role="status" aria-live="polite" aria-label="Roll result">{e ? `${e.label} ${e.total}. ${e.breakdown}` : ""}</p>
        {e && (
          <div className="toast pr-result" data-testid="roll-result">
            <div className="pr-result-text">
              <span className="pr-result-head"><span className="pr-result-label">{e.label}</span> <strong className="pr-result-total">{e.total}</strong></span>
              <span className="pr-result-dice">{e.breakdown}</span>
            </div>
            <span className="pr-result-cmds">
              {shown.action && <Button small variant="primary" onClick={shown.action.run}>{shown.action.label}</Button>}
              <Button small onClick={() => setHistory(true)}>History</Button>
              <IconButton icon="x" label="Dismiss roll" onClick={onDismiss} />
            </span>
          </div>
        )}
      </div>
      {history && e && <RollHistoryDialog characterId={e.characterId} onClose={() => setHistory(false)} />}
    </OverlayPortal>
  );
}

function HistoryRow({ r }: { r: RollEntry }) {
  return (
    <li className="v5-row static pr-history-row">
      <span className="grow">{r.label}<span className="v5-note" style={{ display: "block" }}>{r.breakdown}</span></span>
      <span className="meta">{time(r.at)}</span>
      <span className="v5-count">{r.total}</span>
    </li>
  );
}

/** The session's rolls of this character, newest first; Clear empties them. Not saved with the character. */
export function RollHistoryDialog({ characterId, onClose }: { characterId: string; onClose: () => void }) {
  const rolls = useRolls(characterId);
  const footer = (<>
    <Button disabled={rolls.length === 0} onClick={() => clearRolls(characterId)}>Clear</Button>
    <Button variant="primary" onClick={onClose}>Close</Button>
  </>);
  return (
    <Modal title="Recent rolls" onClose={onClose} footer={footer}>
      {rolls.length === 0
        ? <EmptyState title="No rolls yet" />
        : <ul className="v5-list pr-history" aria-label="Recent rolls">{rolls.map((r) => <HistoryRow key={r.id} r={r} />)}</ul>}
      <p className="v5-note">Rolls are kept until the app closes. They are not saved with the character.</p>
    </Modal>
  );
}

/** The "Recent rolls" button at the top of the roll tabs. */
export function RollHistoryButton({ characterId }: { characterId: string }) {
  const [open, setOpen] = useState(false);
  const count = useRolls(characterId).length;
  return (<>
    <Button small icon="dice" onClick={() => setOpen(true)}>{`Recent rolls${count ? ` (${count})` : ""}`}</Button>
    {open && <RollHistoryDialog characterId={characterId} onClose={() => setOpen(false)} />}
  </>);
}
