import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { recorderClient } from "../../data";
import { describeError } from "../../data/errors";
import { useRecorder } from "../../data/useRecorder";
import { navigate, routes } from "../../lib/router";
import { inTauri } from "../../lib/windows";
import { invoke } from "@tauri-apps/api/core";
import { Button, Sheet, useToast } from "../../ui";
import { RecordingPopover } from "../Recording/RecordingPopover";
import { usePlatform } from "../../lib/platform";
import { useT } from "../../lib/i18n/I18nProvider";
import { useStore } from "../../data/StoreProvider";
import { savedText } from "../../lib/eta";

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
  const t = useT();
  const store = useStore();
  const platform = usePlatform();
  const [open, setOpen] = useState(false);
  const [micDenied, setMicDenied] = useState(false);
  const active = rec.state.state !== "idle";

  useEffect(() => {
    if (rec.notice && !open && active) toast(rec.notice.text);
  }, [rec.notice]);

  // In the app the recording controls live in the menu bar popover; in a browser, a floating panel (`begin`).

  const begin = useCallback(async () => {
    try {
      await recorderClient.start();
      if (inTauri()) void invoke("tray_toggle");
      else setOpen(true);
    } catch (e) {
      toast(describeError(e));
    }
  }, [toast]);

  // Stable between renders (the main window subscribes its event listeners with these).
  const start = useCallback(async () => {
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
  }, [active, begin]);

  /** After Stop: say the recording is safe and how long the transcript will take, then open the meeting. */
  const announceSaved = useCallback(
    async (id?: string) => {
      const meeting = id ? await store.getMeeting(id).catch(() => undefined) : undefined;
      toast(savedText(t, meeting?.eta));
      if (id) navigate(routes.meeting(id));
    },
    [store, t, toast],
  );

  const stop = async () => {
    try {
      const id = await recorderClient.stop();
      setOpen(false);
      await announceSaved(id || undefined);
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
              toast(t("shell.discarded"));
            }}
            onOpenSettings={(pane) => recorderClient.openSettings(pane)}
            onDismissNotice={rec.clearNotice}
          />
        </div>
      )}
      <Sheet open={micDenied} title={t("shell.micOffTitle")} onClose={() => setMicDenied(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("shell.micOffHeading")}</h2>
          <p className="m-0 text-[13px] text-subtle">
            {t("shell.micOffBody", { path: platform.micSettingsPath })}
          </p>
          <div className="flex flex-wrap justify-end gap-2">
            <Button
              onClick={async () => {
                setMicDenied(false);
                await begin();
              }}
            >
              {t("shell.recordCallOnly")}
            </Button>
            {/* Fixing the permission is what most people want, so it's the main button. */}
            <Button variant="primary" onClick={() => recorderClient.openSettings("microphone")}>
              {platform.openSettings}
            </Button>
          </div>
        </div>
      </Sheet>
    </>
  );

  return { state: rec.state, active, open, start, announceSaved, panel };
}
