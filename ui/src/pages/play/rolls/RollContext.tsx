import { type ReactNode, createContext, useCallback, useContext, useMemo, useState } from "react";
import { type DiceExpr, type DiceResult, type RollMode, critical, rollD20, rollExpr } from "../../../lib/dice";
import { type RollEntry, addRoll, currentDie } from "./rollStore";
import { RollResultCard } from "./RollResult";

/** Dice rolled against snapshot numbers: a damage roll, a hit die, a resource die. */
export interface DiceRequest {
  label: string;
  expr: DiceExpr;
  /** Damage dice never count below this (the attack's `dieFloor`). */
  floor?: number | null;
}

/** A d20 test: check, save, skill, initiative or attack roll. `damage` makes it an attack that offers its damage next. */
export interface D20Request {
  label: string;
  modifier: number;
  mode: RollMode;
  critRange?: number;
  damage?: DiceRequest | null;
}

export interface RollApi {
  d20: (r: D20Request) => void;
  /** Rolls, records and shows the dice; returns the result (a spent hit die sends its face to the host). */
  dice: (r: DiceRequest & { critical?: boolean }) => DiceResult;
}

/** What the result card shows: the newest roll and, after an attack roll, its damage roll. */
export interface Shown { entry: RollEntry; action: { label: string; run: () => void } | null }

const Ctx = createContext<RollApi | null>(null);

function rollDice(characterId: string, r: DiceRequest & { critical?: boolean }): { entry: RollEntry; result: DiceResult } {
  const expr = r.critical ? critical(r.expr) : r.expr;
  const result = rollExpr(expr, currentDie(), { floor: r.floor, label: r.critical ? "critical" : undefined });
  return { entry: addRoll({ characterId, label: r.label, total: result.total, breakdown: result.breakdown }), result };
}

/** Rolls for one character's Play tab, with the result card where the user is looking. */
export function RollProvider({ characterId, children }: { characterId: string; children: ReactNode }) {
  const [shown, setShown] = useState<Shown | null>(null);
  const dice = useCallback((r: DiceRequest & { critical?: boolean }) => {
    const { entry, result } = rollDice(characterId, r);
    setShown({ entry, action: null });
    return result;
  }, [characterId]);
  const d20 = useCallback((r: D20Request) => {
    const result = rollD20(r.modifier, r.mode, currentDie(), { critRange: r.critRange });
    const damage = r.damage ?? null;
    const crit = damage !== null && result.isCrit;
    const followUp = damage ? { label: crit ? `Roll critical damage` : "Roll damage", critical: crit } : null;
    const entry = addRoll({ characterId, label: r.label, total: result.total, breakdown: result.breakdown, followUp });
    const action = damage && followUp
      ? { label: followUp.label, run: () => { dice({ ...damage, label: crit ? `${damage.label}, critical` : damage.label, critical: crit }); } }
      : null;
    setShown({ entry, action });
  }, [characterId, dice]);
  const api = useMemo(() => ({ d20, dice }), [d20, dice]);
  const dismiss = useCallback(() => setShown(null), []);
  return (
    <Ctx.Provider value={api}>
      {children}
      <RollResultCard shown={shown} onDismiss={dismiss} />
    </Ctx.Provider>
  );
}

export function useRoller(): RollApi {
  const api = useContext(Ctx);
  if (!api) throw new Error("useRoller needs a RollProvider");
  return api;
}
