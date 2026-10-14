import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { recorderClient } from "../../data";
import { onEvent } from "../../data/tauri/invoke";
import { useRecorder } from "../../data/useRecorder";
import { useFitWindow, useTransparentPage } from "../../lib/floatingWindow";
import { openSettings } from "../../lib/windows";
import { AlwaysBanner, CallEndedPrompt, Nudge } from "../Recording/CallPrompts";

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

  useEffect(() => {
    void invoke<Prompt | null>("calls_prompt").then(setPrompt);
    return onEvent<Prompt | null>("call-prompt", setPrompt);
  }, []);

  if (!prompt) return null;
  const stop = () => void invoke("calls_stop").catch(() => {});
  const left = prompt.deadline ? Math.max(1, Math.round((prompt.deadline - Date.now()) / 1000)) : 30;

  return (
    <div ref={ref} className="inline-block">
      {prompt.kind === "nudge" && <Nudge app={prompt.label} onRecord={() => void invoke("calls_accept", { appId: prompt.app })} onDismiss={() => void invoke("calls_dismiss", { appId: prompt.app })} />}
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
