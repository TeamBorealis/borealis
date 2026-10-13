import type { AiInstallConsent, AiLoginHelp, AiSetupEvent, AiSetupProvider, AiSetupProviderId, AiSetupStatus } from "../types";
import { apiError } from "./errors";

/** Exact pinned installs, as in the Rust side (never a floating tag). */
const PACKAGES: Record<AiSetupProviderId, string> = { claude: "@anthropic-ai/claude-code@2.1.293", codex: "@openai/codex@0.161.0" };
const LOGIN: Record<AiSetupProviderId, string> = { claude: "claude auth login", codex: "codex login" };

export interface MockSetupInit {
  installed: AiSetupProviderId[];
  loggedIn: AiSetupProviderId[];
  hasNpm: boolean;
  completed: boolean;
  mobile: boolean;
  /** Simulates the user declining the backend's native confirmation. */
  declineNativeConfirm: boolean;
}

/** In-memory first-run setup: installing needs a one-time token, exactly like the Rust side. */
export class MockAiSetup {
  private s: MockSetupInit;
  private tokens = new Map<string, AiSetupProviderId>();
  private seq = 0;
  constructor(private emit: (e: AiSetupEvent) => void, init: Partial<MockSetupInit> = {}) {
    this.s = { installed: [], loggedIn: [], hasNpm: true, completed: true, mobile: false, declineNativeConfirm: false, ...init };
  }

  status(): AiSetupStatus {
    const providers: AiSetupProvider[] = (["claude", "codex"] as const).map((backend) => {
      const installed = this.s.installed.includes(backend);
      return {
        backend, installed,
        path: installed ? `/usr/local/bin/${backend}` : null,
        version: installed ? "1.0.0" : null,
        loggedIn: installed ? this.s.loggedIn.includes(backend) : null,
        installCommand: `npm install -g ${PACKAGES[backend]}`,
        loginCommand: LOGIN[backend],
      };
    });
    return { providers, npmPath: this.s.hasNpm ? "/usr/local/bin/npm" : null, profile: { state: "verified", detail: [] }, completed: this.s.completed, mobile: this.s.mobile };
  }

  confirm(provider: AiSetupProviderId): AiInstallConsent {
    if (!this.s.hasNpm) throw apiError("requirement_failed", "npm was not found. Install Node.js first.");
    const token = `consent-${++this.seq}`;
    this.tokens.set(token, provider);
    return { token, command: `npm install -g ${PACKAGES[provider]}` };
  }

  async install(provider: AiSetupProviderId, token: string): Promise<void> {
    if (this.tokens.get(token) !== provider) throw apiError("requirement_failed", "Installation needs your explicit confirmation first.");
    this.tokens.delete(token);
    if (this.s.declineNativeConfirm) throw apiError("requirement_failed", "Installation was cancelled.");
    this.emit({ kind: "output", provider, line: `npm install -g ${PACKAGES[provider]}` });
    this.emit({ kind: "output", provider, line: "added 1 package" });
    this.s = { ...this.s, installed: [...this.s.installed, provider] };
    this.emit({ kind: "done", provider, ok: true, message: "Installed." });
  }

  login(provider: AiSetupProviderId): AiLoginHelp {
    // The mock "signs in" as soon as the user asks, so a re-check shows the result.
    this.s = { ...this.s, loggedIn: [...new Set([...this.s.loggedIn, provider])] };
    return { command: LOGIN[provider], opened: false };
  }

  complete(): void { this.s = { ...this.s, completed: true }; }
}
