import { useCallback } from "react";
import { isApiError } from "../api/contract";
import { useConfirm } from "../components";

const DECLINED = "Imported content stays off Codex. Install or sign in to Claude in AI setup, then try again.";

/**
 * Runs an AI action that handles outside content (imports, the LAN host). When only Codex is available the
 * backend answers `consent_required`; the user is asked once and the action is retried with the flag set.
 * Declining rejects with a `consent_required` error whose message says how to use Claude instead.
 */
export function useWithCodexConsent(): <T>(run: (allowUntrustedWithCodex: boolean) => Promise<T>) => Promise<T> {
  const confirm = useConfirm();
  return useCallback(async <T,>(run: (allow: boolean) => Promise<T>): Promise<T> => {
    try {
      return await run(false);
    } catch (e) {
      if (!isApiError(e) || e.code !== "consent_required") throw e;
      const answer = await confirm({
        title: "Use Codex with imported content?",
        confirmLabel: "Continue with Codex",
        cancelLabel: "Use Claude",
        body: (
          <>
            <p>Codex can run shell commands and read files on this computer if the imported content tells it to.</p>
            <p className="muted small">Claude does not have that risk and is used automatically when it is installed. Continue only if you trust this content.</p>
          </>
        ),
      });
      if (answer !== "confirm") throw { code: "consent_required", message: DECLINED };
      return run(true);
    }
  }, [confirm]);
}
