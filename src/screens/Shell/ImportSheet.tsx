import { FileAudio } from "lucide-react";
import { Button, Label, Sheet } from "../../ui";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function ImportSheet({ open, onClose }: Props) {
  return (
    <Sheet open={open} title="Import audio" onClose={onClose}>
      <div className="flex flex-col gap-4 p-5">
        <div>
          <h2 className="m-0 text-lg font-extrabold tracking-tight">Import audio</h2>
          <p className="m-0 mt-1 text-[13px] text-subtle">
            Drop a Voice Memo or a meeting recording. It becomes a meeting with a transcript and summary.
          </p>
        </div>
        <div className="grid place-items-center gap-2 rounded-card border-[1.5px] border-dashed border-input bg-surface px-4 py-10 text-center">
          <FileAudio size={28} aria-hidden="true" className="text-subtle" />
          <span className="text-[13px] font-semibold">Drag an audio file here</span>
          <Label>.m4a · .mp3 · .wav · .mp4</Label>
          <Button>Choose file…</Button>
        </div>
        <p className="m-0 text-xs text-subtle">
          One track is labeled “Them” unless you split it by speaker later. The file stays on this Mac.
        </p>
        <div className="flex justify-end">
          <Button onClick={onClose}>Cancel</Button>
        </div>
      </div>
    </Sheet>
  );
}
