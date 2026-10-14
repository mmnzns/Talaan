import type { Settings } from "../../data/settings";

export interface TabProps {
  settings: Settings;
  update: (patch: Partial<Settings>) => Promise<Settings>;
}
