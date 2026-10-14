import type { Speaker } from "../data/types";
import { cn } from "./cn";

/** Short, colored speaker pill used in the transcript and speaker panel. */
export function SpeakerTag({ speaker, name, className }: { speaker: Speaker; name: string; className?: string }) {
  const tone = toneFor(speaker);
  return (
    <span
      title={name}
      className={cn(
        "block overflow-hidden text-ellipsis whitespace-nowrap rounded-full border px-1 py-0.5 text-center font-mono text-[11px] font-semibold leading-[14px]",
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
  if (n === 0) return "border-line bg-sunken text-subtle";
  const slot = ((n - 1) % 4) + 1;
  return [
    "border-sp1-line bg-sp1 text-sp1-fg",
    "border-sp2-line bg-sp2 text-sp2-fg",
    "border-sp3-line bg-sp3 text-sp3-fg",
    "border-sp4-line bg-sp4 text-sp4-fg",
  ][slot - 1];
}

const shortName = (name: string) => name.split(" ")[0];
