import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { useRef } from "react";
import { recorderClient } from "../../data";
import { describeError } from "../../data/errors";
import { useRecorder } from "../../data/useRecorder";
import { useFitWindow, useTransparentPage } from "../../lib/floatingWindow";
import { useToast } from "../../ui";
import type { NextMeeting } from "../../data/settings";
import { appOf, eventOf } from "../../lib/nextMeeting";
import { MenuBarPopover } from "../Recording/MenuBarPopover";
import { RecordingPopover } from "../Recording/RecordingPopover";

const hide = () => invoke("tray_hide");

/** Opens something in the main window and closes the popover. */
async function inMain(event: string, payload?: unknown) {
  await invoke("app_show_main");
  await emit(event, payload);
  await hide();
}

/** The menu bar popover window (`#/tray`): idle menu, or the recording controls while recording. */
export function TrayWindow() {
  useTransparentPage();
  const ref = useRef<HTMLDivElement>(null);
  const rec = useRecorder(recorderClient);
  const toast = useToast();
  const active = rec.state.state !== "idle";
  useFitWindow(ref, active);

  // `next`: started from the calendar's next meeting, so the recording takes its title, attendees and call app.
  const start = async (next?: NextMeeting) => {
    const perm = await recorderClient.microphonePermission().catch(() => "unknown" as const);
    // A denied microphone is explained in the main window, which has room for it.
    if (perm === "denied" || perm === "restricted") return inMain("record-request");
    if (perm === "undetermined" && !(await recorderClient.requestMicrophone().catch(() => false))) return inMain("record-request");
    try {
      await (next ? recorderClient.start(appOf(next), eventOf(next)) : recorderClient.start());
    } catch (e) {
      toast(describeError(e));
    }
  };

  return (
    <div ref={ref} className="inline-block">
      {active ? (
        <RecordingPopover
          state={rec.state}
          levels={rec.levels}
          notice={rec.notice}
          onPause={() => recorderClient.pause()}
          onResume={() => recorderClient.resume()}
          onMark={(note) => recorderClient.mark(note)}
          onStop={async () => {
            try {
              const id = await recorderClient.stop();
              if (id) await inMain("recording-saved", id);
            } catch (e) {
              toast(describeError(e));
            }
          }}
          onDiscard={async () => {
            await recorderClient.discard();
            await hide();
          }}
          onOpenSettings={(pane) => recorderClient.openSettings(pane)}
          onDismissNotice={rec.clearNotice}
        />
      ) : (
        <MenuBarPopover
          onStart={start}
          onOpenMeeting={(id) => inMain("open-meeting", id)}
          onOpenApp={async () => {
            await invoke("app_show_main");
            await hide();
          }}
          onQuit={() => invoke("app_quit")}
        />
      )}
    </div>
  );
}
