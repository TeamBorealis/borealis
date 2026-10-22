import React from "react";
import { createRoot } from "react-dom/client";
import { HashRouter } from "react-router";
import { createApi } from "./api";
import { App } from "./App";
import { AppProviders } from "./AppProviders";
import { SplashWindow } from "./components/splash/SplashWindow";
import { StartupGate } from "./components/splash/StartupGate";
import { installButtonNames } from "./lib/buttonNames";
import { isTauri } from "./lib/tauri";
import { tauriStartupBridge } from "./lib/startup";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/components.css";
import "./styles/layout.css";
import "./styles/shell.css";
import "./styles/pages.css";
import "./styles/import.css";
import "./styles/play.css";
import "./styles/splash.css";

if (/Android/i.test(navigator.userAgent)) document.documentElement.classList.add("android");
if (isTauri()) installButtonNames();
const root = document.getElementById("root");
if (!root) throw new Error("root element missing");
const app = () => (
  <AppProviders api={createApi()}>
    <HashRouter>
      <App />
    </HashRouter>
  </AppProviders>
);

if (window.location.hash.startsWith("#/splash")) {
  // The desktop splash window: only the progress view, no API or router.
  document.documentElement.classList.add("splash-doc");
  createRoot(root).render(<SplashWindow bridge={tauriStartupBridge} version={__APP_VERSION__} />);
} else {
  // Inside Tauri the backend loads on a thread: hold the app (and, on mobile, show the splash) until it is ready.
  const forceMock = import.meta.env.VITE_USE_MOCK === "1";
  createRoot(root).render(
    <React.StrictMode>
      {isTauri() && !forceMock ? (
        <StartupGate bridge={tauriStartupBridge} version={__APP_VERSION__}>{app()}</StartupGate>
      ) : app()}
    </React.StrictMode>,
  );
}
