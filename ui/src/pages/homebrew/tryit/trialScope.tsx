import { type ReactNode, useMemo } from "react";
import type { Api, CharacterSnapshot, HomebrewTrialView } from "../../../api/contract";
import { useToast } from "../../../components";
import { ApiProvider, useApi } from "../../../state/ApiContext";
import { CharacterScope, type CharacterCtx } from "../../../state/CharacterContext";
import { errorMessage } from "../../../state/useAsync";
import type { TrialSession } from "./trialSession";

/** What a Build picker asks of the character that Try it cannot do: the test character only takes choices. */
export const TRIAL_ONLY_CHOICES = "Try it only makes choices on the test character.";

const notForTrial = (): Promise<never> => Promise.reject({ code: "invalid_argument", message: TRIAL_ONLY_CHOICES });

/** `{ characterId }` arguments: a character command. */
const isCharacterCall = (args: unknown): boolean => typeof args === "object" && args !== null && "characterId" in args;

/**
 * The app's Api as the Build pickers inside Try it see it: a choice's options come from `homebrewTrialOptions`, a pick or a
 * clear becomes a trial change, and every other character command is refused (the open character is never touched).
 * Content commands (element descriptions, settings) pass through.
 */
export function trialApi(base: Api, session: TrialSession): Api {
  const snapshotOf = async (p: Promise<HomebrewTrialView>): Promise<CharacterSnapshot> => (await p).snapshot;
  const own: Partial<Api> = {
    getSlotOptions: ({ characterId, slotId, query, limit, offset }) => base.homebrewTrialOptions({ trialId: characterId, slotId, query, limit, offset }),
    applySelection: ({ slotId, elementId }) => snapshotOf(session.change({ slotId, elementId })),
    clearSelection: ({ slotId }) => snapshotOf(session.change({ slotId, elementId: null })),
    switchVersion: async ({ slotId, elementId }) => ({ snapshot: await snapshotOf(session.change({ slotId, elementId })), dropped: [] }),
    getSnapshot: async () => {
      const view = session.getSnapshot().view;
      if (!view) throw { code: "not_found", message: "The test character is not ready yet." };
      return view.snapshot;
    },
  };
  return new Proxy(base, {
    get(target, prop, receiver) {
      if (typeof prop === "string" && prop in own) return own[prop as keyof Api];
      const value: unknown = Reflect.get(target, prop, receiver);
      if (typeof value !== "function") return value;
      return (...args: unknown[]) => (isCharacterCall(args[0]) ? notForTrial() : (value as (...a: unknown[]) => unknown).apply(target, args));
    },
  });
}

const noop = () => undefined;

/**
 * Runs its children (the Build pickers) against the trial character: an Api that routes choices to the trial, and a
 * character context whose snapshot is the trial's. Nothing here saves, autosaves or changes the open character.
 */
export function TrialScope({ session, view, revision, children }: { session: TrialSession; view: HomebrewTrialView; revision: number; children: ReactNode }) {
  const base = useApi();
  const toast = useToast();
  const api = useMemo(() => trialApi(base, session), [base, session]);
  const trialId = view.trialId;
  const value = useMemo<CharacterCtx>(() => ({
    snapshot: view.snapshot, revision, busy: false,
    open: async () => false, adopt: noop,
    mutate: async (fn) => {
      try { return await fn(api, trialId); } catch (e) { toast.push(errorMessage(e), "error"); return null; }
    },
    save: noop, saveOpen: false, closeSave: noop, commitSave: async () => false, savedByUser: false, close: async () => undefined,
  }), [view.snapshot, revision, api, trialId, toast]);
  return <ApiProvider api={api}><CharacterScope value={value}>{children}</CharacterScope></ApiProvider>;
}
