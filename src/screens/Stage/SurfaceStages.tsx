import { useEffect, useMemo, useState, type ReactNode } from "react";
import { MockRecorderClient } from "../../data/recorder";
import type { NextMeeting } from "../../data/settings";
import { useRecorder } from "../../data/useRecorder";
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

/** A sample calendar meeting starting `inMin` minutes from now (negative: already started). */
function sampleNext(inMin: number): NextMeeting {
  const start = Date.now() + inMin * 60_000 - 30_000;
  return {
    title: "Design review: onboarding",
    startMs: start,
    endMs: start + 30 * 60_000,
    attendees: ["Ana Reyes", "Sam Okafor"],
    emails: ["ana.reyes@example.com", "sam.okafor@example.com"],
    joinUrl: "https://us02web.zoom.us/j/81234567890",
    appId: "zoom",
    appLabel: "Zoom",
  };
}

export function MenuBarStage() {
  return (
    <div>
      <Caption
        title="Menu bar"
        note="What opens from the Talaan icon in the menu bar when nothing is recording: with the calendar's next meeting (soon, in progress) and without one."
      />
      <div className="flex flex-wrap items-start gap-8">
        <Frame label="next meeting soon">
          <MenuBarPopover next={sampleNext(12)} onStart={() => navigate(routes.screens("recording"))} />
        </Frame>
        <Frame label="meeting in progress">
          <MenuBarPopover next={sampleNext(-5)} onStart={() => navigate(routes.screens("recording"))} />
        </Frame>
        <Frame label="no calendar or nothing coming up">
          <MenuBarPopover next={null} onStart={() => navigate(routes.screens("recording"))} />
        </Frame>
      </div>
    </div>
  );
}

export function RecordingStage() {
  const toast = useToast();
  const client = useMemo(() => new MockRecorderClient({ startAtMs: 768_000 }), []);
  const rec = useRecorder(client);
  const [speakers, setSpeakers] = useState(true);

  useEffect(() => {
    client.start();
    return () => {
      client.stop();
    };
  }, [client]);

  return (
    <div>
      <Caption
        title="Recording"
        note="The menu bar popover while a call is being recorded (simulated here). The toolbar Record button runs the real recorder."
      />
      <div className="flex flex-wrap items-start gap-8">
        {rec.state.state !== "idle" ? (
          <RecordingPopover
            state={rec.state}
            levels={rec.levels}
            notice={rec.notice}
            app="Zoom"
            onPause={() => client.pause()}
            onResume={() => client.resume()}
            onMark={(n) => client.mark(n)}
            onStop={() => {
              client.stop();
              toast("Saved. Transcribing now…");
            }}
            onDiscard={() => {
              client.discard();
              toast("Recording discarded");
            }}
            onOpenSettings={() => toast("Opens the system settings")}
            onDismissNotice={rec.clearNotice}
          />
        ) : (
          <Button variant="primary" onClick={() => client.start()}>
            Start again
          </Button>
        )}
        <div className="flex max-w-xs flex-col gap-3">
          <Label>try a state</Label>
          <label className="flex items-center justify-between gap-3 text-[13px]">
            On the MacBook speakers
            <Switch
              aria-label="On the MacBook speakers"
              checked={speakers}
              onChange={(v) => {
                setSpeakers(v);
                client.setSpeakers(v);
              }}
            />
          </label>
          <Button onClick={() => client.notice({ text: "Switched to MacBook Pro Microphone. Recording continues.", kind: "info" })}>Simulate AirPods disconnecting</Button>
          <Button
            onClick={() =>
              client.notice({ text: "No call audio is coming through. If the other people are talking, allow Talaan under Screen & System Audio Recording.", kind: "warning", settingsPane: "system-audio" })
            }
          >
            Simulate blocked system audio
          </Button>
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
