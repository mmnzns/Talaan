import { useEffect, useState } from "react";
import type { Route } from "../../lib/router";
import { openSettings } from "../../lib/windows";
import { AskScreen } from "../Ask/AskScreen";
import { ImportSheet } from "../Shell/ImportSheet";
import { MeetingList } from "../MeetingList/MeetingList";
import { MeetingPane } from "../Meeting/MeetingPane";
import { useRecording } from "./RecordingPanel";
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

  const selectedId = route.name === "meeting" ? route.id : route.name === "ask" ? route.meetingId : undefined;

  return (
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
  );
}
