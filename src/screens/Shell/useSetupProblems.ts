import { useEffect, useState } from "react";
import { recorderClient } from "../../data";
import { useSettings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import { checkSetup, needsFixing, probeSetup, type Probe } from "../../lib/setupCheck";

/** How many Status checks stop a call right now (the dot on the Settings button). The computer is looked at when
 *  the window opens or comes to the front; settings changes (a finished download, a new key) are judged against
 *  that look at once, so the dot goes away without waiting. Never prompts: call audio is never tested here. */
export function useSetupProblems(): number {
  const store = useSettingsStore();
  const { settings } = useSettings();
  const [probe, setProbe] = useState<Probe>();
  // What the look itself depends on: the local server's address and the calendar and folder rows.
  const key = settings ? [settings.summaryEngine, settings.ollamaAddress, settings.calendarConnected, settings.autoSaveFolder].join("|") : "";

  useEffect(() => {
    let live = true;
    const look = () => void probeSetup(store, recorderClient).then((p) => live && setProbe(p), () => {});
    look();
    window.addEventListener("focus", look);
    return () => {
      live = false;
      window.removeEventListener("focus", look);
    };
  }, [store, key]);

  if (!probe || !settings) return 0;
  return needsFixing(checkSetup({ ...probe, settings })).length;
}
