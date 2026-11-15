import { act, screen, waitFor, within } from "@testing-library/react";
import { expect, vi } from "vitest";
import type { HomebrewProblem } from "../api/contract";
import type { MockApi } from "../api/mock/MockApi";

// Shared steps of the HC18 creator-message tests (item I3v_g): the engine checks held back, the status line, and the
// assertion that a confirmation stays while a check runs and takes the count once it answers.

const warning = (message: string): HomebrewProblem => ({ severity: "warning", message, path: null, elementId: null, code: "sweep" });
/** What a held check finds once released: two warnings. */
export const LATE = [warning("At level 7 the choice has nothing to offer."), warning("At level 9 the choice has nothing to offer.")];

export interface LateChecks {
  /** The next checks wait until `release`. */
  hold: () => void;
  /** Checks waiting now. */
  pending: () => number;
  /** Lets the waiting checks answer; from now on every check finds `LATE`. */
  release: () => void;
  /** Checks answered after a release. */
  answered: () => number;
}

/** Holds the engine checks back on demand (`homebrewCheckDraft`), as the HC18 save test does. */
export function lateChecks(api: MockApi): LateChecks {
  const check = api.homebrewCheckDraft.bind(api);
  let held = false;
  let late = false;
  let answered = 0;
  const waiting: (() => void)[] = [];
  vi.spyOn(api, "homebrewCheckDraft").mockImplementation(async (args) => {
    if (held) await new Promise<void>((r) => waiting.push(r));
    if (!late) return check(args);
    answered += 1;
    return LATE;
  });
  return {
    hold: () => { held = true; },
    pending: () => waiting.length,
    release: () => {
      held = false;
      late = true;
      waiting.splice(0).forEach((r) => r());
    },
    answered: () => answered,
  };
}

export const editor = () => screen.getByRole("region", { name: "Element editor" });
export const nameBox = () => screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement;
/** The status bar's live line. */
export const line = () => within(screen.getByRole("contentinfo", { name: "Status" })).getByRole("status");
/** PROBLEMS' header ("Problems (2)"). */
export const problemsButton = () => screen.getAllByRole("button", { name: /^Problems \(\d+\)/ })[0]!;
const problemsCount = (): number => Number(/^Problems \((\d+)\)/.exec(problemsButton().textContent ?? "")?.[1] ?? NaN);
const countWords = (n: number): string => `${n} ${n === 1 ? "problem" : "problems"}`;
/** The plain problems line (no message), or "Checking...". */
export const PROBLEMS_LINE = /^(Checking\.\.\.|No problems|\d+ problems?)$/;

/** The line's first part: the message, without the count read after it. */
const message = (): string | null => line().firstElementChild?.textContent ?? line().textContent;

/**
 * The message shows (with the count the element already had, if any); a held check runs (`trigger` starts one when the
 * action itself did not) and the message is still there, never "Checking..."; once the check answers its two warnings
 * are counted after it ("<message> · N problems"), and the message's node is the same one (screen readers hear the count
 * only).
 */
export async function expectKeptThenCounted(checks: LateChecks, text: string, trigger?: () => Promise<void>): Promise<void> {
  await waitFor(() => expect(message()).toBe(text));
  if (trigger) await trigger();
  await waitFor(() => expect(checks.pending()).toBeGreaterThan(0));
  expect(message()).toBe(text);
  const node = line().firstElementChild;
  const before = problemsCount();
  checks.release();
  await waitFor(() => expect(problemsCount()).toBe(before + LATE.length));
  await waitFor(() => expect(line().textContent).toBe(`${text} · ${countWords(before + LATE.length)}`));
  expect(line().firstElementChild).toBe(node);
}

/** How long an answered check is given to reach the status line. */
const SETTLE_MS = 200;

/** A message about the pack, not the open form (multi-select, SPELL LISTS): it stays when a check answers, with no count. */
export async function expectKeptUncounted(checks: LateChecks, text: string, trigger?: () => Promise<void>): Promise<void> {
  await waitFor(() => expect(line().textContent).toBe(text));
  if (trigger) await trigger();
  await waitFor(() => expect(checks.pending()).toBeGreaterThan(0));
  expect(line().textContent).toBe(text);
  checks.release();
  await waitFor(() => expect(checks.answered()).toBeGreaterThan(0));
  // the answer reaches the screen (the editor may be hidden behind SPELL LISTS, so there is no PROBLEMS to wait for)
  await act(() => new Promise<void>((r) => setTimeout(r, SETTLE_MS)));
  expect(line().textContent).toBe(text);
}
