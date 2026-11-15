import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HomebrewDraftView, HomebrewForm, HomebrewProblem, HomebrewSaveResult } from "../api/contract";
import { notImplemented } from "../api/mock/homebrew";
import { DraftSession, type DraftApi, mergeProblems } from "../pages/homebrew/draftSession";
import { HISTORY_LIMIT, record, redo, startHistory, undo } from "../pages/homebrew/history";

// Homebrew WB-09: useDraft's core (spec 10.1-10.2, 8.1). Autosave is debounced (300 ms), one update is in flight at a time,
// stale answers are dropped, typing groups into one undo step per pause (700 ms) or field, the engine checks run after a
// 1.5 s pause, and leaving sends what is pending.

const form = (name: string, extra: Record<string, unknown> = {}): HomebrewForm => ({ kind: "spell", fields: { name, ...extra }, blocks: [], children: [], preserved: [] });

const view = (draftId: string, f: HomebrewForm, problems: HomebrewProblem[] = [], elementId: string | null = null): HomebrewDraftView => ({
  draftId, packId: "homebrew-tst", elementId, form: f, preview: null, problems, updatedAt: "2026-11-10T10:00:00Z", baseChanged: false,
  generated: { json: "{}", tree: [] },
});

const problem = (message: string, severity: HomebrewProblem["severity"] = "error", code = "form.required"): HomebrewProblem =>
  ({ severity, message, path: "fields.name", elementId: null, code });

interface Deferred<T> { promise: Promise<T>; resolve: (v: T) => void; reject: (e: unknown) => void }
function deferred<T>(): Deferred<T> {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

/** A backend whose answers the test releases by hand. */
function fakeApi() {
  const updates: { args: { draftId: string; form: HomebrewForm }; answer: Deferred<HomebrewDraftView> }[] = [];
  const checks: { draftId: string; answer: Deferred<HomebrewProblem[]> }[] = [];
  const api: DraftApi = {
    homebrewUpdateDraft: vi.fn((args: { draftId: string; form: HomebrewForm }) => {
      const answer = deferred<HomebrewDraftView>();
      updates.push({ args, answer });
      return answer.promise;
    }),
    homebrewCheckDraft: vi.fn(({ draftId }: { draftId: string }) => {
      const answer = deferred<HomebrewProblem[]>();
      checks.push({ draftId, answer });
      return answer.promise;
    }),
    homebrewSaveDraft: vi.fn(async ({ draftId }: { draftId: string }): Promise<HomebrewSaveResult> => ({
      pack: { id: "homebrew-tst", name: "Test", version: "1.0.1", kind: "homebrew", rulesets: ["dnd2024"], elementCount: 1, enabled: true, license: "none", attribution: "", sourceIds: [] },
      elementIds: [`ID_TST_SPELL_${draftId.toUpperCase()}`],
    })),
  };
  /** Answers the oldest unanswered update with a view of the form it carried. */
  const answerNext = async (problems: HomebrewProblem[] = []) => {
    const u = updates.find((x) => !(x as { done?: boolean }).done)!;
    (u as { done?: boolean }).done = true;
    u.answer.resolve(view(u.args.draftId, u.args.form, problems));
    await flushMicrotasks();
  };
  return { api, updates, checks, answerNext };
}

async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve();
}

describe("WB-09 undo history (spec 10.2)", () => {
  it("typing in one field is one step until a 700 ms pause", () => {
    // Arrange
    let h = startHistory("", 0);

    // Act
    h = record(h, "F", { group: "field:name", now: 100 });
    h = record(h, "Fi", { group: "field:name", now: 300 });
    h = record(h, "Fir", { group: "field:name", now: 900 }); // 600 ms after the last key: same step
    h = record(h, "Fire", { group: "field:name", now: 1700 }); // 800 ms pause: a new step

    // Assert
    expect(h.past).toEqual(["", "Fir"]);
    expect(undo(h).present).toBe("Fir");
    expect(undo(undo(h)).present).toBe("");
  });

  it("another field, or a plain edit, starts a new step", () => {
    // Arrange
    let h = startHistory("a", 0);

    // Act
    h = record(h, "ab", { group: "field:name", now: 10 });
    h = record(h, "abc", { group: "field:school", now: 20 });
    h = record(h, "abcd", { now: 30 });
    h = record(h, "abcde", { now: 40 });

    // Assert
    expect(h.past).toEqual(["a", "ab", "abc", "abcd"]);
  });

  it("undo then redo walks the same states; a new edit drops the redo states", () => {
    // Arrange
    let h = startHistory(1, 0);
    h = record(h, 2, { now: 1 });
    h = record(h, 3, { now: 2 });

    // Act
    const back = undo(undo(h));
    const forward = redo(back);
    const branched = record(back, 9, { now: 3 });

    // Assert
    expect(back.present).toBe(1);
    expect(forward.present).toBe(2);
    expect(branched.future).toEqual([]);
    expect(redo(branched)).toBe(branched);
    expect(undo(startHistory(0))).toEqual(startHistory(0));
  });

  it("keeps the last 200 steps", () => {
    // Arrange
    let h = startHistory(0, 0);

    // Act
    for (let i = 1; i <= HISTORY_LIMIT + 50; i++) h = record(h, i, { now: i });

    // Assert
    expect(h.past).toHaveLength(HISTORY_LIMIT);
    expect(h.past[0]).toBe(50);
  });

  it("typing right after an undo is a new step, not merged into the undone one", () => {
    // Arrange
    let h = startHistory("", 0);
    h = record(h, "a", { group: "field:name", now: 0 });
    h = record(h, "ab", { group: "field:name", now: 100 });

    // Act
    h = undo(h);
    h = record(h, "x", { group: "field:name", now: 150 });

    // Assert
    expect(h.past).toEqual([""]);
    expect(h.present).toBe("x");
  });
});

describe("WB-09 draft session: autosave, undo, conflicts", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  async function opened(problems: HomebrewProblem[] = []) {
    const fake = fakeApi();
    const s = new DraftSession(fake.api);
    await s.open(view("d1", form("Ember"), problems));
    return { ...fake, s };
  }

  it("autosaves 300 ms after the last edit, once, with the latest form", async () => {
    // Arrange
    const { s, updates, api } = await opened();

    // Act
    s.setField("name", "Ember B", true);
    vi.advanceTimersByTime(200);
    s.setField("name", "Ember Bo", true);
    vi.advanceTimersByTime(299);
    const before = updates.length;
    vi.advanceTimersByTime(1);

    // Assert
    expect(before).toBe(0);
    expect(api.homebrewUpdateDraft).toHaveBeenCalledTimes(1);
    expect(updates[0]!.args).toEqual({ draftId: "d1", form: form("Ember Bo") });
    expect(s.getSnapshot().autosave).toBe("saving");
  });

  it("an edit while an autosave is in flight is sent after it answers, and the answer never overwrites the screen", async () => {
    // Arrange
    const { s, updates, answerNext } = await opened();
    s.setField("name", "Ember Bolt", true);
    vi.advanceTimersByTime(300);

    // Act: type more while the first update is in flight
    s.setField("name", "Ember Bolts", true);
    vi.advanceTimersByTime(300);
    const whileInFlight = updates.length;
    await answerNext([problem("Old answer")]);
    const formAfterFirst = s.form;
    vi.advanceTimersByTime(300);
    await answerNext();

    // Assert
    expect(whileInFlight).toBe(1);
    expect(formAfterFirst).toEqual(form("Ember Bolts"));
    expect(updates.map((u) => u.args.form.fields.name)).toEqual(["Ember Bolt", "Ember Bolts"]);
    expect(s.getSnapshot().autosave).toBe("saved");
    expect(s.getSnapshot().view?.problems).toEqual([]);
  });

  it("an answer for a draft that is no longer open is dropped", async () => {
    // Arrange
    const { s, updates } = await opened();
    s.setField("name", "Ember Bolt");
    vi.advanceTimersByTime(300);

    // Act: open another draft while d1's update is in flight; d1's answer arrives late
    const opening = s.open(view("d2", form("Frost")));
    updates[0]!.answer.resolve(view("d1", form("Ember Bolt"), [problem("From d1")]));
    await opening;
    await flushMicrotasks();

    // Assert
    expect(s.draftId).toBe("d2");
    expect(s.form).toEqual(form("Frost"));
    expect(s.getSnapshot().view?.problems).toEqual([]);
    expect(s.getSnapshot().history?.past).toEqual([]);
  });

  it("opening another draft first sends what the previous one had pending", async () => {
    // Arrange
    const { s, updates } = await opened();
    s.setField("name", "Ember Bolt", true);

    // Act: no debounce wait
    const opening = s.open(view("d2", form("Frost")));
    await flushMicrotasks();
    updates[0]!.answer.resolve(view("d1", form("Ember Bolt")));
    await opening;

    // Assert
    expect(updates[0]!.args).toEqual({ draftId: "d1", form: form("Ember Bolt") });
    expect(s.draftId).toBe("d2");
  });

  it("undo back to what was autosaved sends nothing more", async () => {
    // Arrange
    const { s, api, answerNext } = await opened();
    s.setField("level", 3);
    vi.advanceTimersByTime(300);
    await answerNext();

    // Act
    s.setField("level", 4);
    s.undo();
    vi.advanceTimersByTime(1000);

    // Assert
    expect(api.homebrewUpdateDraft).toHaveBeenCalledTimes(1);
    expect(s.form?.fields.level).toBe(3);
    expect(s.getSnapshot().autosave).toBe("saved");
  });

  it("undo and redo are autosaved like any edit", async () => {
    // Arrange
    const { s, updates, answerNext } = await opened();
    s.setField("school", "Evocation");
    vi.advanceTimersByTime(300);
    await answerNext();

    // Act
    s.undo();
    vi.advanceTimersByTime(300);
    await answerNext();
    s.redo();
    vi.advanceTimersByTime(300);
    await answerNext();

    // Assert
    expect(updates.map((u) => u.args.form.fields.school ?? null)).toEqual(["Evocation", null, "Evocation"]);
  });

  it("a failed autosave keeps the form, says so, and the next edit tries again", async () => {
    // Arrange
    const { s, updates } = await opened();
    s.setField("name", "Ember Bolt");
    vi.advanceTimersByTime(300);

    // Act
    updates[0]!.answer.reject({ code: "not_found", message: "That draft was closed." });
    await flushMicrotasks();
    const failed = s.getSnapshot();
    s.setField("name", "Ember Bolt!");
    vi.advanceTimersByTime(300);

    // Assert
    expect(failed.autosave).toBe("error");
    expect(failed.error).toBe("That draft was closed.");
    expect(s.form?.fields.name).toBe("Ember Bolt!");
    expect(updates).toHaveLength(2);
  });

  it("Save sends the pending edit first, then saves; edits wait until the element is opened again", async () => {
    // Arrange
    const { s, api, updates, answerNext } = await opened();
    s.setField("name", "Ember Bolt", true);

    // Act
    const saving = s.save();
    await flushMicrotasks();
    const updatedBeforeSave = updates.length;
    expect(api.homebrewSaveDraft).not.toHaveBeenCalled();
    await answerNext();
    const result = await saving;
    s.setField("name", "Ember Bolt 2");
    vi.advanceTimersByTime(1000);
    const sentWhileDetached = updates.length;
    s.rebase(view("d9", form("Ember Bolt"), [], result.elementIds[0]));
    vi.advanceTimersByTime(300);

    // Assert
    expect(updatedBeforeSave).toBe(1);
    expect(api.homebrewSaveDraft).toHaveBeenCalledWith({ draftId: "d1" });
    expect(sentWhileDetached).toBe(1);
    expect(updates[1]!.args).toEqual({ draftId: "d9", form: form("Ember Bolt 2") });
  });

  it("history survives Save: undo after reopening steps back over the save, into the new draft", async () => {
    // Arrange
    const { s, updates, answerNext } = await opened();
    s.setField("level", 2);
    vi.advanceTimersByTime(300);
    await answerNext();
    await s.save();

    // Act
    s.rebase(view("d2", form("Ember", { level: 2 }), [], "ID_TST_SPELL_EMBER"));
    const afterRebase = s.getSnapshot().autosave;
    s.undo();
    vi.advanceTimersByTime(300);

    // Assert
    expect(afterRebase).toBe("saved");
    expect(updates[1]!.args).toEqual({ draftId: "d2", form: form("Ember") });
  });

  it("after Save the element reopened in the backend's words is what is on screen: nothing is autosaved again", async () => {
    // Arrange: the real backend reads a saved element back with its own order and values (P3 bug: a saved weapon then
    // showed "Unsaved changes" and stayed listed as an unsaved draft)
    const { s, updates, answerNext } = await opened();
    s.setField("level", 2);
    vi.advanceTimersByTime(300);
    await answerNext();
    await s.save();
    const reopened: HomebrewForm = { kind: "spell", fields: { level: 2, name: "Ember", slot: "onehand" }, blocks: [], children: [], preserved: [] };

    // Act
    s.rebase(view("d2", reopened, [], "ID_TST_SPELL_EMBER"));
    vi.advanceTimersByTime(1000);

    // Assert: the form is the reopened one, saved, and no update went out for it
    expect(s.getSnapshot().autosave).toBe("saved");
    expect(s.form).toEqual(reopened);
    expect(updates).toHaveLength(1);
    // Undo still steps back over the save
    s.undo();
    vi.advanceTimersByTime(300);
    expect(updates[1]!.args).toEqual({ draftId: "d2", form: form("Ember") });
  });

  it("Save refused by the backend (errors) keeps the draft open and reports why", async () => {
    // Arrange
    const { s, api } = await opened([problem("Give it a name.")]);
    vi.mocked(api.homebrewSaveDraft).mockRejectedValueOnce({ code: "invalid_argument", message: "Fix the problems marked in red before saving." });

    // Act
    const outcome = await s.save().then(() => "saved", () => "refused");
    s.setField("name", "Fixed");
    vi.advanceTimersByTime(300);

    // Assert
    expect(outcome).toBe("refused");
    expect(s.getSnapshot().error).toBe("Fix the problems marked in red before saving.");
    expect(s.getSnapshot().saving).toBe(false);
    expect(api.homebrewUpdateDraft).toHaveBeenCalledTimes(1);
  });

  it("runs the engine checks after a 1.5 s pause and merges their problems; an edit first cancels them", async () => {
    // Arrange
    const { s, checks, answerNext } = await opened();
    s.setField("name", "Ember Bolt");
    vi.advanceTimersByTime(300);
    await answerNext([problem("A spell needs a school.", "error", "MissingRequiredSetter")]);

    // Act
    vi.advanceTimersByTime(1000);
    s.setField("name", "Ember Bolt!");
    vi.advanceTimersByTime(300);
    const checksAfterEdit = checks.length;
    await answerNext([problem("A spell needs a school.", "error", "MissingRequiredSetter")]);
    vi.advanceTimersByTime(1500);
    checks[0]!.answer.resolve([problem("A spell needs a school.", "error", "MissingRequiredSetter"), problem("At level 7 the choice has nothing to offer.", "warning", "sweep")]);
    await flushMicrotasks();
    const snap = s.getSnapshot();

    // Assert
    expect(checksAfterEdit).toBe(0);
    expect(checks).toHaveLength(1);
    expect(mergeProblems(snap.view!.problems, snap.engineProblems).map((p) => p.message)).toEqual(["A spell needs a school.", "At level 7 the choice has nothing to offer."]);
    expect(snap.checking).toBe(false);
  });

  it("engine problems of an older form go with the next autosave answer and with Save's reopen", async () => {
    // Arrange
    const { s, checks, answerNext } = await opened();
    const running = s.check();
    checks[0]!.answer.resolve([problem("At level 7 the choice has nothing to offer.", "warning", "sweep")]);
    await running;
    const checked = s.getSnapshot().engineProblems.length;

    // Act
    s.setField("name", "Ember Bolt");
    const whileEditing = s.getSnapshot().engineProblems.length;
    vi.advanceTimersByTime(300);
    await answerNext();
    const afterAnswer = s.getSnapshot().engineProblems.length;
    const again = s.check();
    checks[1]!.answer.resolve([problem("Still there.", "warning", "sweep")]);
    await again;
    await s.save();
    s.rebase(view("d2", form("Ember Bolt"), [], "ID_TST_SPELL_EMBER_BOLT"));

    // Assert
    expect(checked).toBe(1);
    expect(whileEditing).toBe(1);
    expect(afterAnswer).toBe(0);
    expect(s.getSnapshot().engineProblems).toEqual([]);
  });

  it("an engine check the backend does not have yet shows no engine problems", async () => {
    // Arrange
    const { s, checks } = await opened();

    // Act
    const running = s.check();
    const during = s.getSnapshot().checking;
    checks[0]!.answer.reject(notImplemented("homebrew_check_draft"));
    await running;

    // Assert
    expect(during).toBe(true);
    expect(s.getSnapshot().engineProblems).toEqual([]);
    expect(s.getSnapshot().checking).toBe(false);
  });

  it("leaving sends the pending edit at once (closing asks nothing and loses nothing)", async () => {
    // Arrange
    const { s, updates } = await opened();
    s.setField("name", "Ember Bolt", true);

    // Act
    s.dispose();
    await flushMicrotasks();

    // Assert
    expect(updates).toHaveLength(1);
    expect(updates[0]!.args.form.fields.name).toBe("Ember Bolt");
  });

  it("typing groups into one undo step; a pick in another field is its own step", async () => {
    // Arrange
    const { s } = await opened();

    // Act
    s.setField("name", "E", true);
    vi.advanceTimersByTime(100);
    s.setField("name", "Em", true);
    s.setField("school", "Evocation");
    s.undo();
    const afterOne = s.form;
    s.undo();

    // Assert
    expect(afterOne).toEqual(form("Em"));
    expect(s.form).toEqual(form("Ember"));
    expect(s.getSnapshot().history?.future).toHaveLength(2);
  });
});
