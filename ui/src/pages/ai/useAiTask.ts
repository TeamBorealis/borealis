import { useCallback, useEffect, useRef, useState } from "react";
import type { AiEvent, AiMessage, AiMode, CitedAnswerData, HomebrewDraft } from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";
import { asCitedAnswer } from "./citations";

export interface AiRun {
  messages: AiMessage[];
  streaming: boolean;
  error: string | null;
  draft: HomebrewDraft | null;
  /** Validated Rules Q&A results, keyed by the index of the assistant message they belong to. */
  answers: Readonly<Record<number, CitedAnswerData>>;
  send: (mode: AiMode, prompt: string, characterId: string | null, params?: Record<string, unknown>) => Promise<void>;
  cancel: () => void;
  reset: () => void;
}

/** Drives one streaming AI conversation: sends a task, folds `delta` events into the last assistant message. */
export function useAiTask(): AiRun {
  const api = useApi();
  const [messages, setMessages] = useState<AiMessage[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<HomebrewDraft | null>(null);
  const [answers, setAnswers] = useState<Readonly<Record<number, CitedAnswerData>>>({});
  const taskRef = useRef<string | null>(null);
  const answerIndexRef = useRef(0);

  const startingRef = useRef(false);
  const bufferRef = useRef<AiEvent[]>([]);

  const apply = useCallback((e: AiEvent) => {
    if (e.kind === "delta") {
      setMessages((cur) => cur.map((m, i) => (i === cur.length - 1 && m.role === "assistant" ? { ...m, text: m.text + e.text } : m)));
    } else if (e.kind === "result") {
      if (e.result.kind === "homebrew") setDraft(e.result);
      else if (e.result.kind === "rules_qa") {
        const cited = asCitedAnswer(e.result.data);
        const at = answerIndexRef.current;
        if (cited) setAnswers((cur) => ({ ...cur, [at]: cited }));
      }
    }
    else if (e.kind === "error") { setError(e.message); setStreaming(false); }
    else if (e.kind === "done") { setStreaming(false); taskRef.current = null; }
  }, []);

  // Events can arrive before the start command resolves: buffer them until the task id is known.
  useEffect(() => api.subscribe("ai:event", (e) => {
    if (startingRef.current) { bufferRef.current.push(e); return; }
    if (e.taskId === taskRef.current) apply(e);
  }), [api, apply]);

  const send = useCallback(async (mode: AiMode, prompt: string, characterId: string | null, params?: Record<string, unknown>) => {
    const history = messages;
    setError(null);
    setDraft(null);
    setStreaming(true);
    setMessages([...history, { role: "user", text: prompt }, { role: "assistant", text: "" }]);
    answerIndexRef.current = history.length + 1;
    startingRef.current = true;
    bufferRef.current = [];
    try {
      const { taskId } = await api.aiStartTask({ request: { mode, prompt, characterId, history, ...(params ? { params } : {}) } });
      taskRef.current = taskId;
      startingRef.current = false;
      for (const e of bufferRef.current.filter((x) => x.taskId === taskId)) apply(e);
      bufferRef.current = [];
    } catch (e) {
      startingRef.current = false;
      setError(errorMessage(e));
      setStreaming(false);
    }
  }, [api, messages, apply]);

  const cancel = useCallback(() => {
    if (taskRef.current) void api.aiCancelTask({ taskId: taskRef.current });
  }, [api]);

  const reset = useCallback(() => { setMessages([]); setDraft(null); setAnswers({}); setError(null); }, []);
  return { messages, streaming, error, draft, answers, send, cancel, reset };
}
