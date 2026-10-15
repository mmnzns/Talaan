import { FileAudio } from "lucide-react";
import { useEffect, useState } from "react";
import { describeError } from "../../data/errors";
import { onEvent } from "../../data/tauri/invoke";
import { navigate, routes } from "../../lib/router";
import { inTauri } from "../../lib/windows";
import { Button, Label, Sheet } from "../../ui";

interface Props {
  open: boolean;
  /** A file dropped on the main window: import it right away. */
  dropped?: string;
  onClose: () => void;
}

type ImportEvent = { state: "progress"; progress: number } | { state: "done"; id: string } | { state: "failed"; message: string };
type Phase = { kind: "idle" } | { kind: "working"; name: string; progress: number } | { kind: "failed"; message: string };

const fileName = (path: string) => path.split("/").pop() ?? path;

export function ImportSheet({ open, dropped, onClose }: Props) {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  useEffect(() => {
    if (!inTauri()) return;
    return onEvent<ImportEvent>("import-audio", (e) => {
      if (e.state === "progress") setPhase((p) => (p.kind === "working" ? { ...p, progress: e.progress } : p));
      if (e.state === "failed") setPhase({ kind: "failed", message: e.message });
      if (e.state === "done") {
        setPhase({ kind: "idle" });
        onClose();
        navigate(routes.meeting(e.id));
      }
    });
  }, [onClose]);

  const start = async (path: string) => {
    setPhase({ kind: "working", name: fileName(path), progress: 0 });
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("meetings_import", { path }).catch((e) => setPhase({ kind: "failed", message: describeError(e) }));
  };

  useEffect(() => {
    if (dropped) void start(dropped);
  }, [dropped]);

  const choose = async () => {
    if (!inTauri()) return setPhase({ kind: "failed", message: "Importing works in the Talaan app." });
    const { invoke } = await import("@tauri-apps/api/core");
    const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
    const extensions = await invoke<string[]>("import_extensions");
    const path = await openDialog({ multiple: false, directory: false, filters: [{ name: "Audio", extensions }] });
    if (typeof path === "string") await start(path);
  };

  const close = () => {
    if (phase.kind !== "working") setPhase({ kind: "idle" });
    onClose();
  };

  return (
    <Sheet open={open} title="Import audio" onClose={close}>
      <div className="flex flex-col gap-4 p-5">
        <div>
          <h2 className="m-0 text-lg font-extrabold tracking-tight">Import audio</h2>
          <p className="m-0 mt-1 text-[13px] text-subtle">
            Drop a Voice Memo or a meeting recording. It becomes a meeting with a transcript and summary.
          </p>
        </div>
        <div className="grid place-items-center gap-2 rounded-card border border-dashed border-input bg-surface px-4 py-10 text-center">
          <FileAudio size={28} aria-hidden="true" className="text-subtle" />
          {phase.kind === "working" ? (
            <div className="flex w-64 flex-col gap-2" role="status">
              <span className="truncate text-[13px] font-semibold">Reading {phase.name}…</span>
              <span className="h-[5px] overflow-hidden rounded-full bg-line">
                <span className="block h-full bg-ink" style={{ width: `${Math.round(phase.progress * 100)}%` }} />
              </span>
            </div>
          ) : (
            <>
              <span className="text-[13px] font-semibold">Drag an audio file here</span>
              <Label>.m4a · .mp3 · .wav · .aiff · .flac</Label>
              <Button onClick={() => void choose()}>Choose file…</Button>
            </>
          )}
          {phase.kind === "failed" && (
            <span role="alert" className="text-xs font-semibold text-danger">
              {phase.message}
            </span>
          )}
        </div>
        <p className="m-0 text-xs text-subtle">
          One track is labeled “Them” unless you split it by speaker later. The file stays on this computer.
        </p>
        <div className="flex justify-end">
          <Button onClick={close}>{phase.kind === "working" ? "Hide" : "Cancel"}</Button>
        </div>
      </div>
    </Sheet>
  );
}
