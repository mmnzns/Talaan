import { useEffect, useState } from "react";
import { inTauri } from "./windows";
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

/**
 * "About 8 min left", "Less than a minute left", "Almost done". With `progress` at 100% the step isn't over yet
 * (telling speakers apart runs after the last chunk), so it says "Finishing up" instead of a time.
 */
export function etaText(t: T, eta: Eta | undefined, now?: number, progress?: number): string | undefined {
  const left = eta && timeLeft(eta, now);
  if (!left) return undefined;
  if ((progress ?? 0) >= 1) return t("common.etaFinishing");
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

/**
 * Whether a "Summary ready" / "Transcript ready" notification will reach the screen: the setting is on and the
 * OS hasn't turned Talaan's notifications off. Undefined until known (and in a browser, where there are none).
 */
export function useNotificationsAllowed(): boolean | undefined {
  const [allowed, setAllowed] = useState<boolean | undefined>();
  useEffect(() => {
    if (!inTauri()) return;
    void import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<boolean | null>("notifications_allowed"))
      .then((v) => setAllowed(v ?? undefined))
      .catch(() => {});
  }, []);
  return allowed;
}

export const PROCESSING_KEYS = { transcribing: "shell.statusTranscribing", summarizing: "shell.statusSummarizing" } as const satisfies Record<string, MessageKey>;
