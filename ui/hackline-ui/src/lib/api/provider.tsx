import * as React from "react";
import type { ApiClient } from "@hackline/client";

const Ctx = React.createContext<ApiClient | null>(null);

export function ApiProvider({
  client,
  children,
}: {
  client: ApiClient;
  children: React.ReactNode;
}) {
  return <Ctx.Provider value={client}>{children}</Ctx.Provider>;
}

export function useApi(): ApiClient {
  const c = React.useContext(Ctx);
  if (!c) throw new Error("useApi must be used inside <ApiProvider>");
  return c;
}
