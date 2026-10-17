import { useEffect, useState } from "react";
import { onWindows } from "../../lib/platform";
import type { Route } from "../../lib/router";
import { navigate, routes } from "../../lib/router";
import { inTauri, openSettings } from "../../lib/windows";
import { call, onEvent } from "../../data/tauri/invoke";
import { AskScreen } from "../Ask/AskScreen";
import { ImportSheet } from "../Shell/ImportSheet";
import { MeetingList } from "../MeetingList/MeetingList";
import { MeetingPane } from "../Meeting/MeetingPane";
import { RecordingContext, useRecording } from "./RecordingPanel";
import { Toolbar } from "./Toolbar";
import { SetupDownloadPill } from "../Onboarding/SetupDownload";
import { TrayTip } from "../Onboarding/TrayTip";
import { TestRecordingWatcher } from "../Onboarding/TestRecording";

export function MainWindow({ route }: { route: Route }) {
  const [importing, setImporting] = useState(false);
  const [dropped, setDropped] = useState<string>();
  const recording = useRecording();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((onWindows ? e.ctrlKey : e.metaKey) && e.key === ",") {
        e.preventDefault();
        openSettings();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Other windows ask the main window to do things: open a meeting (notification click, menu bar), record
  // (the menu bar when the microphone needs explaining), open Settings (menu bar icon's menu).
  useEffect(() => {
    if (!inTauri()) return;
    const offs = [
      onEvent<string>("open-meeting", (id) => navigate(routes.meeting(id))),
      // Stop in the menu bar: the toast with the time estimate shows here, where the meeting opens.
      onEvent<string>("recording-saved", (id) => void recording.announceSaved(id)),
      onEvent("record-request", () => recording.start()),
      onEvent("open-settings", () => void openSettings()),
    ];
    return () => offs.forEach((off) => off());
  }, [recording.start, recording.announceSaved]);

  // Started by a notification click while Talaan wasn't running (Windows): open that meeting.
  useEffect(() => {
    if (!inTauri()) return;
    void call<string | null>("meetings_launch_link")
      .then((id) => id && navigate(routes.meeting(id)))
      .catch(() => {});
  }, []);

  // Drop an audio file anywhere on the window to import it.
  useEffect(() => {
    if (!inTauri()) return;
    let off: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((e) => {
          if (e.payload.type === "over") setImporting(true);
          if (e.payload.type === "drop" && e.payload.paths[0]) {
            setImporting(true);
            setDropped(e.payload.paths[0]);
          }
        })
        .then((unlisten) => (off = unlisten)),
    );
    return () => off?.();
  }, []);

  const selectedId = route.name === "meeting" ? route.id : route.name === "ask" ? route.meetingId : undefined;

  return (
    <RecordingContext.Provider value={{ start: recording.start }}>
    <div className="flex h-full flex-col bg-paper">
      <Toolbar onImport={() => setImporting(true)} onRecord={recording.start} recording={recording.state} />
      <TrayTip />
      <div className="flex min-h-0 flex-1">
        <MeetingList selectedId={route.name === "meeting" ? route.id : undefined} />
        <main className="flex min-w-0 flex-1 flex-col bg-paper">
          {route.name === "ask" ? (
            <AskScreen meetingId={selectedId} />
          ) : route.name === "meeting" ? (
            <MeetingPane id={route.id} jumpTo={route.t} />
          ) : (
            <MeetingPane />
          )}
        </main>
      </div>
      <ImportSheet
        open={importing}
        dropped={dropped}
        onClose={() => {
          setImporting(false);
          setDropped(undefined);
        }}
      />
      {recording.panel}
      <SetupDownloadPill />
      <TestRecordingWatcher />
    </div>
    </RecordingContext.Provider>
  );
}
