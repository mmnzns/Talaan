import { createContext, useContext, type ReactNode } from "react";
import type { MeetingStore } from "./store";
import type { SettingsStore } from "./settings";

interface Stores {
  meetings: MeetingStore;
  settings: SettingsStore;
}

const StoreContext = createContext<Stores | null>(null);

export function StoreProvider({ meetings, settings, children }: Stores & { children: ReactNode }) {
  return <StoreContext.Provider value={{ meetings, settings }}>{children}</StoreContext.Provider>;
}

function useStores(): Stores {
  const ctx = useContext(StoreContext);
  if (!ctx) throw new Error("StoreProvider is missing");
  return ctx;
}

export const useStore = (): MeetingStore => useStores().meetings;
export const useSettingsStore = (): SettingsStore => useStores().settings;
