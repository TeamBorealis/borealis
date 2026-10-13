import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { COMMANDS, EVENTS, type Api, type ApiError, type ApiMethods, type CommandName, type EventName } from "./contract";

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function toApiError(e: unknown): ApiError {
  if (typeof e === "object" && e !== null && "code" in e && "message" in e) return e as ApiError;
  return { code: "internal", message: typeof e === "string" ? e : String(e) };
}

const NOT_READY = /state not managed/i;
const READY_RETRIES = 100;
const READY_DELAY_MS = 100;

/**
 * Android runs the Rust setup hook (data dir lookup, content extraction) concurrently with the
 * first page load, so early commands can arrive before the app state is managed. Retry those.
 */
async function invokeWhenReady(inv: Invoke, cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await inv(cmd, args);
    } catch (e) {
      const text = typeof e === "string" ? e : toApiError(e).message;
      if (!NOT_READY.test(text) || attempt >= READY_RETRIES) throw e;
      await new Promise((r) => setTimeout(r, READY_DELAY_MS));
    }
  }
}

/** Builds the real Api on top of Tauri `invoke`/`listen` (injectable for tests). */
export function createTauriApi(
  inv: Invoke = invoke as Invoke,
  listenFn: typeof listen = listen,
): Api {
  const methods = {} as Record<string, unknown>;
  for (const name of Object.keys(COMMANDS) as CommandName[]) {
    methods[name] = (args?: Record<string, unknown>) => invokeWhenReady(inv, COMMANDS[name], args).catch((e) => Promise.reject(toApiError(e)));
  }
  return {
    ...(methods as unknown as ApiMethods),
    kind: "tauri",
    subscribe(event: EventName, handler) {
      let disposed = false;
      let unlisten: (() => void) | null = null;
      void listenFn(EVENTS[event], (e) => handler(e.payload as never)).then((u) => {
        if (disposed) u();
        else unlisten = u;
      });
      return () => {
        disposed = true;
        unlisten?.();
      };
    },
  } as Api;
}
