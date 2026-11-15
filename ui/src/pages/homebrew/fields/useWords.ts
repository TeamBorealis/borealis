import { useApi } from "../../../state/ApiContext";
import { useAsync } from "../../../state/useAsync";
import { useCreatorEnv } from "../env";

/** One page holds every word (types and setter names are few). */
const WORD_PAGE = 500;

/**
 * The words a pseudo picker type offers (spec 4.12, WB-19): `["Element Type"]` gives the open types loaded content uses,
 * `["Setter Name", "<type>"]` the setter names elements of that type use. Empty while loading, when `types` is empty, or
 * when the backend can't say: the fields then take free text.
 */
export function useWords(types: readonly string[]): string[] {
  const api = useApi();
  const env = useCreatorEnv();
  const key = types.join("\u0000");
  const words = useAsync(async () => {
    if (types.length === 0) return [];
    try {
      const page = await api.homebrewReferenceOptions({ target: { types: [...types] }, query: "", filters: {}, page: { offset: 0, limit: WORD_PAGE }, ...(env.draftId ? { draftId: env.draftId } : {}) });
      return page.items.map((r) => r.name);
    } catch {
      return [];
    }
  }, [api, env.draftId, key]); // `key` stands for `types`
  return words.data ?? [];
}
