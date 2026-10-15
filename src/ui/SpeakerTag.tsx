import type { Speaker } from "../data/types";
import { cn } from "./cn";

/** Short, colored speaker pill used in the transcript and speaker panel. */
export function SpeakerTag({ speaker, name, className }: { speaker: Speaker; name: string; className?: string }) {
  const tone = toneFor(speaker);
  return (
    <span
      title={name}
      className={cn(
        "block overflow-hidden text-ellipsis whitespace-nowrap rounded-md border px-1 py-[3px] text-center text-[11px] font-bold leading-[14px]",
        tone,
        className,
      )}
    >
      {shortName(name)}
    </span>
  );
}

export function toneFor(speaker: Speaker): string {
  if (speaker === "me") return "border-strong bg-strong text-strong-fg";
  const n = Number(speaker.split(":")[1]);
  if (n === 0) return "border-transparent bg-sp1 text-sp1-fg";
  const slot = ((n - 1) % 4) + 1;
  return [
    "border-transparent bg-sp1 text-sp1-fg",
    "border-transparent bg-sp2 text-sp2-fg",
    "border-transparent bg-sp3 text-sp3-fg",
    "border-transparent bg-sp4 text-sp4-fg",
  ][slot - 1];
}

const shortName = (name: string) => name.split(" ")[0];
