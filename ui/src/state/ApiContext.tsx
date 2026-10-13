import { type ReactNode, createContext, useContext } from "react";
import type { Api } from "../api/contract";

const Ctx = createContext<Api | null>(null);

export function ApiProvider({ api, children }: { api: Api; children: ReactNode }) {
  return <Ctx.Provider value={api}>{children}</Ctx.Provider>;
}

export function useApi(): Api {
  const api = useContext(Ctx);
  if (!api) throw new Error("useApi must be used inside <ApiProvider>");
  return api;
}
