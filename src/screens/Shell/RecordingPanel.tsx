import { createContext, useContext, useEffect, useState } from "react";
import { recorderClient } from "../../data";
import { describeError } from "../../data/errors";
import { useRecorder } from "../../data/useRecorder";
import { navigate, routes } from "../../lib/router";
import { inTauri } from "../../lib/windows";
import { invoke } from "@tauri-apps/api/core";
import { Button, Sheet, useToast } from "../../ui";
import { RecordingPopover } from "../Recording/RecordingPopover";

/** Lets screens inside the main window start a recording (e.g. the empty state's "Record a call"). */
export const RecordingContext = createContext<{ start: () => void } | null>(null);
export const useStartRecording = () => useContext(RecordingContext)?.start;

/**
 * The recording controls until M6 moves them into the real menu bar: a floating panel under the toolbar.
 * Also owns "start": checks the microphone permission first and explains a denial.
 */
export function useRecording() {
  const rec = useRecorder(recorderClient);
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [micDenied, setMicDenied] = useState(false);
  const active = rec.state.state !== "idle";

  useEffect(() => {
    if (rec.notice && !open && active) toast(rec.notice.text);
  }, [rec.notice]);

  // In the app the recording controls live in the menu bar popover; in a browser, a floating panel.
  const showControls = () => (inTauri() ? void invoke("tray_toggle") : setOpen(true));

  const begin = async () => {
    try {
      await recorderClient.start();
      showControls();
    } catch (e) {
      toast(describeError(e));
    }
  };

  const start = async () => {
    if (active) {
      if (inTauri()) void invoke("tray_toggle");
      else setOpen((o) => !o);
      return;
    }
    const perm = await recorderClient.microphonePermission().catch(() => "unknown" as const);
    if (perm === "undetermined") {
      const ok = await recorderClient.requestMicrophone().catch(() => false);
      if (!ok) return setMicDenied(true);
    } else if (perm === "denied" || perm === "restricted") {
      return setMicDenied(true);
    }
    await begin();
  };

  const stop = async () => {
    try {
      const id = await recorderClient.stop();
      setOpen(false);
      toast("Saved");
      if (id) navigate(routes.meeting(id));
    } catch (e) {
      toast(describeError(e));
    }
  };

  const panel = (
    <>
      {active && open && (
        <div className="fixed right-4 top-[60px] z-40">
          <RecordingPopover
            state={rec.state}
            levels={rec.levels}
            notice={rec.notice}
            onPause={() => recorderClient.pause()}
            onResume={() => recorderClient.resume()}
            onMark={(note) => recorderClient.mark(note)}
            onStop={stop}
            onDiscard={async () => {
              await recorderClient.discard();
              setOpen(false);
              toast("Recording discarded");
            }}
            onOpenSettings={(pane) => recorderClient.openSettings(pane)}
            onDismissNotice={rec.clearNotice}
          />
        </div>
      )}
      <Sheet open={micDenied} title="Microphone is off" onClose={() => setMicDenied(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 text-lg font-extrabold tracking-tight">Talaan can’t use the microphone</h2>
          <p className="m-0 text-[13px] text-subtle">
            Your side of the call won’t be recorded. Turn Talaan on in System Settings › Privacy &amp; Security › Microphone, or record only the other people for now.
          </p>
          <div className="flex flex-wrap justify-end gap-2">
            <Button onClick={() => recorderClient.openSettings("microphone")}>Open System Settings</Button>
            <Button
              variant="primary"
              onClick={async () => {
                setMicDenied(false);
                await begin();
              }}
            >
              Record call audio only
            </Button>
          </div>
        </div>
      </Sheet>
    </>
  );

  return { state: rec.state, active, open, start, panel };
}
