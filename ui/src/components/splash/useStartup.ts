import { useEffect, useState } from "react";
import type { StartupBridge, StartupProgress, StartupSignal } from "../../lib/startup";

export interface StartupState {
  progress: StartupProgress | null;
  done: boolean;
}

/** Follows the backend's startup events; `ready` tells it this webview is listening (it replays). */
export function useStartup(bridge: StartupBridge, ready: StartupSignal): StartupState {
  const [state, setState] = useState<StartupState>({ progress: null, done: false });
  useEffect(() => {
    let live = true;
    const unlisten: Array<() => void> = [];
    void Promise.all([
      bridge.onProgress((progress) => { if (live) setState((s) => ({ ...s, progress })); }),
      bridge.onDone(() => { if (live) setState((s) => ({ ...s, done: true })); }),
    ]).then((fns) => {
      if (!live) { fns.forEach((f) => f()); return; }
      unlisten.push(...fns);
      bridge.signal(ready);
    });
    return () => { live = false; unlisten.forEach((f) => f()); };
  }, [bridge, ready]);
  return state;
}
