import { useEffect, useState } from "react";
import type { Route } from "../../lib/router";
import { navigate, routes } from "../../lib/router";
import { inTauri, openSettings } from "../../lib/windows";
import { onEvent } from "../../data/tauri/invoke";
import { AskScreen } from "../Ask/AskScreen";
import { ImportSheet } from "../Shell/ImportSheet";
import { MeetingList } from "../MeetingList/MeetingList";
import { MeetingPane } from "../Meeting/MeetingPane";
import { RecordingContext, useRecording } from "./RecordingPanel";
import { Toolbar } from "./Toolbar";

export function MainWindow({ route }: { route: Route }) {
  const [importing, setImporting] = useState(false);
  const recording = useRecording();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === ",") {
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
      onEvent("record-request", () => recording.start()),
      onEvent("open-settings", () => void openSettings()),
    ];
    return () => offs.forEach((off) => off());
  }, [recording.start]);

  const selectedId = route.name === "meeting" ? route.id : route.name === "ask" ? route.meetingId : undefined;

  return (
    <RecordingContext.Provider value={{ start: recording.start }}>
    <div className="flex h-full flex-col bg-paper">
      <Toolbar onImport={() => setImporting(true)} onRecord={recording.start} recording={recording.state} />
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
      <ImportSheet open={importing} onClose={() => setImporting(false)} />
      {recording.panel}
    </div>
    </RecordingContext.Provider>
  );
}
