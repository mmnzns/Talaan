import { useEffect, useMemo, useState } from "react";
import { recorderClient } from "../../data";
import { useSettings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import { lightChecks, needsFixing, probeLight, type LightProbe } from "../../lib/setupCheck";
import { throttled } from "../../lib/throttle";

/** A window coming to the front looks again at most this often. */
const FOCUS_GAP_MS = 30_000;

/** How many checks stop a call right now (the dot on the Settings button). Only the light look (`probeLight`:
 *  microphone access, microphones, the speech model), never the full Status probe: no helper starts, no folder is
 *  walked, nothing goes over the network. It looks when the window opens and when it comes to the front (at most
 *  every 30 s, never twice at once); settings changes (a finished download) are judged against the last look at
 *  once, so the dot goes away without waiting. */
export function useSetupProblems(): number {
  const store = useSettingsStore();
  const { settings } = useSettings();
  const [probe, setProbe] = useState<LightProbe>();

  useEffect(() => {
    let live = true;
    const look = throttled(async () => {
      const p = await probeLight(store, recorderClient);
      if (live) setProbe(p);
    }, FOCUS_GAP_MS);
    const onFocus = () => void look().catch(() => {});
    void look(true).catch(() => {});
    window.addEventListener("focus", onFocus);
    return () => {
      live = false;
      window.removeEventListener("focus", onFocus);
    };
  }, [store]);

  return useMemo(() => (probe && settings ? needsFixing(lightChecks({ ...probe, settings })).length : 0), [probe, settings]);
}
