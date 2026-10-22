import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import type { AiSetupProvider, AiSetupProviderId, AiSetupStatus } from "../../api/contract";
import { Alert, Button, useShell } from "../../components";
import { Expander } from "../../components/controls";
import { Tag, type TagTone } from "../ai/Panes";
import { useApi } from "../../state/ApiContext";
import { errorMessage, useAsync } from "../../state/useAsync";

const NAMES: Record<AiSetupProviderId, { title: string; blurb: string }> = {
  claude: { title: "Claude Code", blurb: "Anthropic's command-line assistant. Sign in with your Claude account." },
  codex: { title: "Codex CLI", blurb: "OpenAI's command-line assistant. Sign in with ChatGPT or an API key." },
};

function Steps({ p, status, onChanged }: { p: AiSetupProvider; status: AiSetupStatus; onChanged: () => void }) {
  const api = useApi();
  const [busy, setBusy] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [login, setLogin] = useState<{ command: string; opened: boolean } | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => api.subscribe("aisetup:event", (e) => {
    if (e.provider !== p.backend) return;
    if (e.kind === "output") setLog((l) => [...l, e.line].slice(-200));
  }), [api, p.backend]);

  // One confirmation only: the desktop app shows a native dialog naming the exact command.
  const install = async () => {
    setBusy(true); setError(null); setLog([]);
    try {
      const consent = await api.aiSetupConfirmInstall({ provider: p.backend });
      await api.aiSetupInstall({ provider: p.backend, consentToken: consent.token });
      onChanged();
    } catch (e) { if (!/cancelled/i.test(errorMessage(e))) setError(errorMessage(e)); } finally { setBusy(false); }
  };

  const showLogin = async () => {
    setError(null);
    try { setLogin(await api.aiSetupLogin({ provider: p.backend, openTerminal: true })); } catch (e) { setError(errorMessage(e)); }
  };

  const copy = async (text: string) => {
    try { await navigator.clipboard.writeText(text); setCopied(true); } catch { setCopied(false); }
  };

  const tone: TagTone = !p.installed ? "" : p.loggedIn ? "ok" : "warn";
  const label = !p.installed ? "Not installed" : p.loggedIn ? "Ready" : p.loggedIn === false ? "Sign-in needed" : "Installed";
  return (
    <section aria-label={NAMES[p.backend].title}>
    <Expander title={NAMES[p.backend].title} headerExtra={<Tag tone={tone}>{label}</Tag>}>
      <div className="v5-inline">
        <p className="v5-note" style={{ margin: 0 }}>{NAMES[p.backend].blurb}</p>
        {p.installed && <p className="small">Found {p.version ?? "a version"} at <code>{p.path}</code>.</p>}
        {!p.installed && status.npmPath && (
          <>
            <p className="muted small">Runs <code>{p.installCommand}</code> from the npm registry. You will be asked to confirm before it starts.</p>
            <div className="row"><Button variant="primary" disabled={busy} onClick={() => void install()}>{busy ? "Installing…" : `Install ${NAMES[p.backend].title}`}</Button></div>
          </>
        )}
        {!p.installed && !status.npmPath && (
          <Alert tone="warning">
            npm was not found, so Borealis cannot install this for you. Install Node.js from nodejs.org, then run <code>{p.installCommand}</code> in a terminal and come back to check again.
          </Alert>
        )}
        {log.length > 0 && <pre className="small" role="log" aria-label={`${NAMES[p.backend].title} install output`} style={{ maxHeight: 160, overflow: "auto" }}>{log.join("\n")}</pre>}
        {p.installed && p.loggedIn !== true && (
          <div className="stack">
            <div className="row"><Button variant="primary" onClick={() => void showLogin()}>Sign in</Button>
              <Button onClick={onChanged}>I have signed in, check again</Button></div>
            {login && (
              <div role="status">
                <p>{login.opened ? "A terminal window opened. If it did not, run this command yourself:" : "Run this command in a terminal and follow the prompts:"}</p>
                <p><code>{login.command}</code> <Button small onClick={() => void copy(login.command)}>{copied ? "Copied" : "Copy command"}</Button></p>
                <p className="muted small">Sign-in happens in the vendor's own tool. Borealis never sees your password or tokens.</p>
              </div>
            )}
          </div>
        )}
        {error && <Alert tone="danger" role="alert">{error}</Alert>}
      </div>
    </Expander>
    </section>
  );
}

export function AiSetupPage() {
  const api = useApi();
  const nav = useNavigate();
  const { setBorealisSettingsOpen } = useShell();
  const status = useAsync(() => api.aiSetupStatus(), [api]);
  const [saving, setSaving] = useState(false);
  const finish = async (skipped: boolean) => {
    setSaving(true);
    try { await api.aiSetupComplete({ skipped }); } finally { setSaving(false); }
    nav("/characters");
  };
  const s = status.data;
  const anyReady = s?.providers.some((p) => p.installed && p.loggedIn === true) ?? false;
  return (
    <div className="v5-setup">
      <div className="v5-sheet">
        <div className="v5-sheet-head">
          <h1>Set up AI assistance</h1>
        </div>
        <div className="v5-sheet-body">
          <p className="v5-note" style={{ marginTop: 0 }}>Borealis can use the Claude or Codex command-line tool you already have to answer rules questions, advise on builds and help with homebrew. Everything else works without it.</p>
          {status.error && <Alert tone="danger" role="alert">{status.error}</Alert>}
          {s?.mobile && <Alert tone="info">Phones and tablets cannot run these tools. Pair with a desktop Borealis from Borealis Settings instead.</Alert>}
          {s && !s.mobile && (
            <>
              {s.providers.map((p) => <Steps key={p.backend} p={p} status={s} onChanged={status.reload} />)}
              {s.profile.state === "repaired" && <Alert tone="info">Borealis restored its AI helper files after they were changed.</Alert>}
              {s.profile.state === "error" && <Alert tone="warning">The Borealis AI helper files could not be installed: {s.profile.detail.join(" ")}</Alert>}
            </>
          )}
        </div>
        <div className="v5-sheet-foot">
          <Button icon="cog" onClick={() => setBorealisSettingsOpen(true)}>Borealis Settings</Button>
          <Button disabled={saving} onClick={() => void finish(true)}>Skip — use Borealis without AI</Button>
          {s && !s.mobile && <Button variant="primary" disabled={saving} onClick={() => void finish(false)}>{anyReady ? "Done" : "Continue without AI for now"}</Button>}
        </div>
      </div>
    </div>
  );
}
