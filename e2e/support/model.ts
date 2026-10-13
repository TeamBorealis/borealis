import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

// Agent steps are optional extras: every test must pass with no model.
// Model source (user choice 2026-10-11): the ChatGPT login (`npx e2e login openai`).
// OpenRouter stays as a fallback when OPENROUTER_API_KEY is set.

const CHATGPT_PROVIDER_ID = 'openai';

function credentialsJson(): string | undefined {
  if (process.env.E2E_OAUTH_CREDENTIALS) return process.env.E2E_OAUTH_CREDENTIALS;
  const configHome = process.env.XDG_CONFIG_HOME ?? join(homedir(), '.config');
  try {
    return readFileSync(join(configHome, 'e2e', 'oauth.json'), 'utf8');
  } catch {
    return undefined;
  }
}

export function hasChatgptLogin(): boolean {
  const json = credentialsJson();
  if (!json) return false;
  try {
    return Boolean(JSON.parse(json)?.[CHATGPT_PROVIDER_ID]);
  } catch {
    return false;
  }
}

export function hasOpenRouterKey(): boolean {
  return Boolean(process.env.OPENROUTER_API_KEY);
}

/** True when agent steps can run. Guard agent steps with `test.skip(!hasModel(), 'no model')`. */
export function hasModel(): boolean {
  return hasChatgptLogin() || hasOpenRouterKey();
}
