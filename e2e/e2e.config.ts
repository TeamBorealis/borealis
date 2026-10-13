import type { E2EConfig } from 'e2e';
import { web } from '@e2e-dev/web';
import { mobile } from '@e2e-dev/mobile';
import { macos } from './engines/macos/index.ts';
import { createOpenRouter } from '@openrouter/ai-sdk-provider';
import { chatgpt } from 'e2e/oauth/chatgpt';
import { hasChatgptLogin, hasOpenRouterKey } from './support/model.js';
import { ANDROID_AVD, ANDROID_BUNDLE_ID, findAndroidApk, findIosApp, IOS_BUNDLE_ID, IOS_SIMULATOR } from './support/mobile-env.js';

// Fixed desktop window size used by the web suites.
const VIEWPORT = { width: 1355, height: 847 } as const;
const PORT = 5179;

// Agent steps are optional extras; every test must pass with NO model (see support/model.ts).
// Primary: the user's ChatGPT login (`npx e2e login openai`). Fallback: OPENROUTER_API_KEY.
// E2E_MODEL overrides the model id; `npx e2e models openai` lists what the plan serves.
const SYSTEM = 'You are a thorough QA agent for Borealis, a character builder. Verify every outcome on screen.';

function chooseModel() {
  if (hasChatgptLogin()) return chatgpt(process.env.E2E_MODEL ?? 'gpt-6-luna');
  if (hasOpenRouterKey()) {
    return createOpenRouter({ apiKey: process.env.OPENROUTER_API_KEY })(
      process.env.E2E_MODEL ?? 'openai/gpt-6-luna-fast',
    );
  }
  return undefined;
}

const model = chooseModel();
const agents = model ? { default: { model, system: SYSTEM } } : undefined;

export default {
  tests: 'tests/**/*.e2e.ts',
  targets: [
    {
      name: 'web',
      engine: web({ viewport: VIEWPORT }),
      app: {
        url: `http://127.0.0.1:${PORT}`,
        command: {
          // MockApi is selected by ui/src/api/index.ts whenever the UI runs outside Tauri;
          // VITE_USE_MOCK=1 makes it explicit.
          executable: process.execPath,
          args: ['node_modules/vite/bin/vite.js', '--port', String(PORT), '--strictPort', '--host', '127.0.0.1'],
          cwd: '../ui',
          env: { VITE_USE_MOCK: '1' },
          reuseExisting: true,
          log: '.e2e/logs/app.log',
          startupTimeout: 90_000,
        },
      },
    },
    {
      // Real Tauri iOS build (Rust backend included) on the simulator. Run via `npm run e2e:ios`.
      // The default scripts select `--target web` so no simulator is ever needed.
      name: 'ios',
      engine: mobile({ platform: 'ios', device: IOS_SIMULATOR }),
      app: { bundleId: IOS_BUNDLE_ID, appPath: findIosApp() },
    },
    {
      // Real Tauri Android debug APK (Rust backend included) on the `borealis` AVD. `npm run e2e:android`.
      name: 'android',
      engine: mobile({ platform: 'android', device: ANDROID_AVD }),
      app: { bundleId: ANDROID_BUNDLE_ID, appPath: findAndroidApk() },
    },
    {
      // The real Tauri .app on macOS via agent-device's macOS helper. `npm run e2e:macos`.
      // Needs Accessibility + Screen Recording for the launching app; backs up and restores ~/Library data.
      name: 'macos',
      engine: macos(),
      app: { bundleId: 'app.borealis', appPath: '../target/release/bundle/macos/Borealis.app' },
    },
  ],
  ...(agents ? { agents } : {}),
  cache: 'read-write',
  retries: 0,
  reporters: ['list'],
} satisfies E2EConfig;
