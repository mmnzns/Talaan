import { useState, type ReactNode } from "react";
import { navigate, routes } from "../../lib/router";
import { openSettings } from "../../lib/windows";
import { Button, Label, Switch, useToast } from "../../ui";
import { MenuBarPopover } from "../Recording/MenuBarPopover";
import { RecordingPopover } from "../Recording/RecordingPopover";
import { AlwaysBanner, CallEndedPrompt, Nudge } from "../Recording/CallPrompts";

function Caption({ title, note }: { title: string; note: string }) {
  return (
    <div className="mb-3 flex flex-col gap-0.5">
      <h2 className="m-0 text-lg font-extrabold tracking-tight">{title}</h2>
      <p className="m-0 max-w-xl text-[13px] text-subtle">{note}</p>
    </div>
  );
}

function Frame({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col items-start gap-2">
      <Label>{label}</Label>
      {children}
    </div>
  );
}

export function MenuBarStage() {
  return (
    <div>
      <Caption title="Menu bar" note="What opens from the Talaan icon in the menu bar when nothing is recording." />
      <MenuBarPopover onStart={() => navigate(routes.screens("recording"))} />
    </div>
  );
}

export function RecordingStage() {
  const toast = useToast();
  const [speakers, setSpeakers] = useState(true);
  const [notice, setNotice] = useState<string>();
  const [instance, setInstance] = useState(0);

  return (
    <div>
      <Caption
        title="Recording"
        note="The menu bar popover while a call is being recorded. Timer, levels, pause, marks and quick notes all work here."
      />
      <div className="flex flex-wrap items-start gap-8">
        <RecordingPopover
          key={instance}
          app="Zoom"
          startAtMs={768_000}
          onBuiltInSpeakers={speakers}
          notice={notice}
          onStop={() => {
            toast("Saved. Transcribing now…");
            navigate(routes.meetings());
          }}
          onDiscard={() => {
            toast("Recording discarded");
            setInstance((n) => n + 1);
          }}
        />
        <div className="flex max-w-xs flex-col gap-3">
          <Label>try a state</Label>
          <label className="flex items-center justify-between gap-3 text-[13px]">
            On the MacBook speakers
            <Switch aria-label="On the MacBook speakers" checked={speakers} onChange={setSpeakers} />
          </label>
          <Button onClick={() => setNotice("Switched to MacBook Pro Microphone. Recording continues.")}>Simulate AirPods disconnecting</Button>
          <Button onClick={() => setNotice(undefined)}>Clear notice</Button>
        </div>
      </div>
    </div>
  );
}

export function PromptsStage() {
  const toast = useToast();
  return (
    <div className="flex flex-col gap-8">
      <Caption title="Prompts" note="Small cards that appear over other apps when a call starts or ends." />
      <Frame label="Ask · a call app starts using the mic">
        <Nudge onRecord={() => navigate(routes.screens("recording"))} onDismiss={() => toast("Dismissed. Talaan won’t ask again for this call.")} />
      </Frame>
      <Frame label="Always · the app is set to record automatically">
        <AlwaysBanner onStop={() => toast("Stopped and saved")} onChangeRule={() => openSettings("apps")} />
      </Frame>
      <Frame label="Call ended · auto-stops after 30 seconds">
        <CallEndedPrompt onStop={() => toast("Stopped and saved")} onKeep={() => toast("Still recording")} />
      </Frame>
    </div>
  );
}
