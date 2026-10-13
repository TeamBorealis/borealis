import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import type { AiMode } from "../../api/contract";
import { Alert, Button, Check, TextArea, useShell } from "../../components";
import { Expander } from "../../components/controls";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import { ImportLink } from "../import/ImportLink";
import { CitedSources, CitedText, type OpenSource } from "./CitedAnswer";
import { HomebrewPreview } from "./HomebrewPreview";
import { DescHead, PaneSplit, Tag } from "./Panes";
import { useAiTask } from "./useAiTask";

const MODES: { id: AiMode; label: string; placeholder: string; lead: string; group: "Chat" | "Tasks" }[] = [
  { id: "chat", label: "Chat", placeholder: "Ask anything about your character or the game...", lead: "Talk it over. Replies stream in as they are written.", group: "Chat" },
  { id: "homebrew", label: "Homebrew", placeholder: "Describe the feat, spell or item you want...", lead: "Describe an element and get an editable draft you can check against the content schema.", group: "Tasks" },
  { id: "rules_qa", label: "Rules Q&A", placeholder: "Ask a rules question, e.g. how does grappling work?", lead: "Short, sourced answers to rules questions.", group: "Tasks" },
  { id: "build_advisor", label: "Build advisor", placeholder: "What should I take at my next level?", lead: "Suggestions for your next choices, based on this character.", group: "Tasks" },
  { id: "backstory", label: "Backstory", placeholder: "A few ideas for your character's past...", lead: "A few ideas for where your character comes from.", group: "Tasks" },
];

export function AiPage() {
  const api = useApi();
  const [params] = useSearchParams();
  const initialMode = MODES.find((m) => m.id === params.get("mode"))?.id ?? "chat";
  const [mode, setMode] = useState<AiMode>(initialMode);
  const [prompt, setPrompt] = useState(params.get("prompt") ?? "");
  const [useChar, setUseChar] = useState(true);
  const { snapshot } = useCharacter();
  const { setBorealisSettingsOpen, openCompendium } = useShell();
  const openSource: OpenSource = (s) => openCompendium({ query: s.name, elementId: s.id });
  const status = useAsync(() => api.aiStatus(), [api]);
  const run = useAiTask();
  const current = MODES.find((m) => m.id === mode)!;
  const ready = status.data?.availability === "ready";
  const submit = () => {
    const text = prompt.trim();
    if (!text || run.streaming || !ready) return;
    setPrompt("");
    void run.send(mode, text, useChar && snapshot ? snapshot.id : null);
  };
  const list = (
    <>
      <Expander title="Assistant" headerExtra={status.data && <Tag tone={ready ? "ok" : "warn"}>{ready ? `${status.data.provider} ready` : "Unavailable"}</Tag>}>
        <ul className="v5-list" aria-label="AI task">
          {(["Chat", "Tasks"] as const).map((g) => (
            <li key={g}>
              <div className="v5-group">{g}</div>
              {MODES.filter((m) => m.group === g).map((m) => (
                <button key={m.id} type="button" className="v5-row" aria-pressed={m.id === mode} onClick={() => { setMode(m.id); run.reset(); }}>
                  <span className="grow">{m.label}</span>
                </button>
              ))}
            </li>
          ))}
        </ul>
      </Expander>
      <p className="v5-note" style={{ marginTop: 10 }}>Everything stays on this device.</p>
      <div style={{ marginTop: 6 }}><ImportLink /></div>
      <div style={{ marginTop: 6 }}><Button icon="cog" onClick={() => setBorealisSettingsOpen(true)}>Borealis Settings</Button></div>
    </>
  );
  const desc = (
    <>
      <DescHead title={current.label} lead={current.lead} />
      {status.data && !ready && (
        <Alert tone="warning">
          {status.data.availability === "needs_pairing"
            ? "AI runs on a paired desktop. Open Borealis Settings to connect to a desktop instance."
            : `${status.data.detail} Install the claude or codex CLI to enable the assistant.`}
        </Alert>
      )}
      <div aria-live="polite" aria-label="Conversation">
        {run.messages.length === 0 && <p className="v5-note">No messages yet.</p>}
        {run.messages.map((m, i) => {
          const cited = run.answers[i];
          const sourced = m.role === "assistant" && mode === "rules_qa";
          return (
            <div key={i}>
              <h2 className={`v5-msg-head ${m.role}`}>{m.role === "user" ? "You" : "Assistant"}</h2>
              <div className={`msg ${m.role} ${run.streaming && i === run.messages.length - 1 ? "cursor" : ""}`} data-role={m.role}>
                {sourced ? <CitedText text={cited?.answerMarkdown ?? m.text} sources={cited?.sources} onOpen={openSource} /> : m.text}
              </div>
              {cited && <CitedSources data={cited} onOpen={openSource} />}
            </div>
          );
        })}
      </div>
      {run.error && <Alert tone="danger">{run.error}</Alert>}
      {run.draft && <HomebrewPreview key={JSON.stringify(run.draft.json)} draft={run.draft} />}
      <form className="v5-compose" onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <TextArea label="Your message" placeholder={current.placeholder} value={prompt} onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submit(); } }} />
        <div className="v5-foot">
          {snapshot && <Check label={`Use ${snapshot.name} as context`} checked={useChar} onChange={(e) => setUseChar(e.target.checked)} />}
          <span className="spacer" style={{ flex: 1 }} />
          {run.streaming ? <Button variant="danger" icon="stop" onClick={run.cancel}>Stop</Button>
            : <Button type="submit" variant="primary" icon="send" disabled={!prompt.trim() || !ready}>Send</Button>}
        </div>
      </form>
    </>
  );
  return (
    <div className="v5-page">
      <h1 className="sr-only">AI assistant</h1>
      <PaneSplit chat list={list} desc={desc} label="AI assistant" />
    </div>
  );
}
