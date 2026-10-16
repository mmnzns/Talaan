import { useEffect, useState } from "react";
import type { MeetingListItem } from "../data/types";
import type { MessageKey, T } from "./i18n";

export type Eta = NonNullable<MeetingListItem["eta"]>;

/** How much is left, in words: whole minutes, under a minute, or past the estimate (still finishing). */
export type Left = { kind: "min"; count: number } | { kind: "soon" } | { kind: "over" };

/** Time left until `finishAt`, counted from `now`. A minute or more past the estimate reads as "almost done". */
export function timeLeft(eta: Eta, now: number = Date.now()): Left | undefined {
  const finish = Date.parse(eta.finishAt);
  if (Number.isNaN(finish)) return undefined;
  const ms = finish - now;
  if (ms <= -60_000) return { kind: "over" };
  if (ms < 60_000) return { kind: "soon" };
  return { kind: "min", count: Math.round(ms / 60_000) };
}

/** "About 8 min left", "Less than a minute left", "Almost done". */
export function etaText(t: T, eta: Eta | undefined, now?: number): string | undefined {
  const left = eta && timeLeft(eta, now);
  if (!left) return undefined;
  if (left.kind === "min") return t("common.etaMin", { count: left.count });
  return t(left.kind === "soon" ? "common.etaSoon" : "common.etaAlmost");
}

/** The toast after Stop: "Recording saved. Writing the transcript now (about 8 min)." */
export function savedText(t: T, eta: Eta | undefined, now?: number): string {
  const left = eta && timeLeft(eta, now);
  if (!left || left.kind === "over") return t("shell.savedWriting");
  if (left.kind === "soon") return t("shell.savedWritingSoon");
  return t("shell.savedWritingMin", { count: left.count });
}

/** The current time, updated every `ms` so countdowns move between progress events. */
export function useNow(ms = 15_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(id);
  }, [ms]);
  return now;
}

export const PROCESSING_KEYS = { transcribing: "shell.statusTranscribing", summarizing: "shell.statusSummarizing" } as const satisfies Record<string, MessageKey>;
