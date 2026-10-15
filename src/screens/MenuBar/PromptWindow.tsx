import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { recorderClient } from "../../data";
import { onEvent } from "../../data/tauri/invoke";
import { useRecorder } from "../../data/useRecorder";
import { useFitWindow, useTransparentPage } from "../../lib/floatingWindow";
import { openSettings } from "../../lib/windows";
import { AlwaysBanner, CallEndedPrompt, Nudge } from "../Recording/CallPrompts";
import { useSettings } from "../../data/hooks";

interface Prompt {
  kind: "nudge" | "banner" | "ended";
  app: string;
  label: string;
  deadline?: number;
}

/** The floating call prompt window (`#/prompt`): Record this call?, Recording [app], Call ended. */
export function PromptWindow() {
  useTransparentPage();
  const ref = useRef<HTMLDivElement>(null);
  const [prompt, setPrompt] = useState<Prompt | null>(null);
  useFitWindow(ref, prompt && `${prompt.kind}:${prompt.app}`);
  const rec = useRecorder(recorderClient);
  const { settings, update } = useSettings();

  // "Always for Zoom": set the rule, then record this call.
  const alwaysFor = async (appId: string) => {
    if (settings) await update({ callApps: settings.callApps.map((a) => (a.id === appId ? { ...a, rule: "always" } : a)) });
    await invoke("calls_accept", { appId });
  };

  useEffect(() => {
    void invoke<Prompt | null>("calls_prompt").then(setPrompt);
    return onEvent<Prompt | null>("call-prompt", setPrompt);
  }, []);

  if (!prompt) return null;
  const stop = () => void invoke("calls_stop").catch(() => {});
  const left = prompt.deadline ? Math.max(1, Math.round((prompt.deadline - Date.now()) / 1000)) : 30;

  return (
    <div ref={ref} className="inline-block pb-3.5 pl-2.5 pr-3.5 pt-2.5">
      {prompt.kind === "nudge" && (
        <Nudge
          app={prompt.label}
          onRecord={() => void invoke("calls_accept", { appId: prompt.app })}
          onDismiss={() => void invoke("calls_dismiss", { appId: prompt.app })}
          onAlways={settings?.callApps.some((a) => a.id === prompt.app && !a.isBrowser) ? () => void alwaysFor(prompt.app) : undefined}
        />
      )}
      {prompt.kind === "banner" && (
        <AlwaysBanner
          app={prompt.label}
          elapsedMs={rec.state.elapsedMs}
          onStop={stop}
          onChangeRule={() => {
            void openSettings("apps");
            void invoke("calls_hide");
          }}
        />
      )}
      {prompt.kind === "ended" && (
        <CallEndedPrompt
          key={prompt.deadline}
          app={prompt.label}
          recordedMs={rec.state.elapsedMs}
          seconds={left}
          total={30}
          onStop={stop}
          onKeep={() => void invoke("calls_keep")}
          onTimeout={() => {}}
        />
      )}
    </div>
  );
}
