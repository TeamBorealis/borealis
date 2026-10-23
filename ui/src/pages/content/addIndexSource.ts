import type { Api } from "../../api/contract";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import { describeIndexUpdate } from "./indexUpdateText";

export interface IndexSourceOutcome { text: string; tone: "info" | "warning" | "error" }

/**
 * What the Indexes tab does for "Fetch": add the index address as a content source, then update so its listed files
 * arrive. Used when an opened or dropped lone `.index` is offered as a source. Never throws: a failure (offline, bad
 * certificate, bad address) comes back as an error outcome.
 */
export async function addIndexSource(api: Api, url: string): Promise<IndexSourceOutcome> {
  try {
    await api.addIndex({ url });
    const r = await api.updateIndexes();
    const list = await api.listIndexes().catch(() => []);
    const text = describeIndexUpdate(r, list, `Last checked at ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`);
    setStatus(text.toast);
    return { text: text.toast, tone: text.tone };
  } catch (e) {
    const text = errorMessage(e);
    setStatus(text);
    return { text, tone: "error" };
  }
}
