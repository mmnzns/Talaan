import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { onEvent } from "../../data/tauri/invoke";
import { recorderClient } from "../../data";
import { describeError } from "../../data/errors";
import { useRecorder } from "../../data/useRecorder";
import { FIT_PAD, useFitWindow, useTransparentPage } from "../../lib/floatingWindow";
import { useToast } from "../../ui";
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
  // The height between the tray icon and the far screen edge (Windows; None on the Mac), so the recording
  // popover scrolls instead of reaching behind the taskbar. Rust sends it again each time the popover opens.
  const [room, setRoom] = useState<number | null>(null);
  useEffect(() => {
    void invoke<number | null>("tray_room").then(setRoom).catch(() => {});
    return onEvent<number>("tray-room", setRoom);
  }, []);

  const start = async () => {
    const perm = await recorderClient.microphonePermission().catch(() => "unknown" as const);
    // A denied microphone is explained in the main window, which has room for it.
    if (perm === "denied" || perm === "restricted") return inMain("record-request");
    if (perm === "undetermined" && !(await recorderClient.requestMicrophone().catch(() => false))) return inMain("record-request");
    try {
      await recorderClient.start();
    } catch (e) {
      toast(describeError(e));
    }
  };

  return (
    <div ref={ref} className="inline-block">
      {active ? (
        <RecordingPopover
          state={rec.state}
          maxHeight={room ? `${Math.floor(room - FIT_PAD)}px` : undefined}
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
