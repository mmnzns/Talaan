/**
 * The pending update, shared by everything in one window (the main window's banner and toolbar chip, the menu
 * bar popover's line, Settings › General). Rust keeps the version the last check found (`updates_pending`) and
 * says when a check finds one (`update-available`), so every window agrees; this module adds what is per
 * window: Later (hides the banner for that version until Talaan restarts) and the install progress.
 */
import { useEffect, useSyncExternalStore } from "react";
import type { T } from "../lib/i18n";
import { inTauri } from "../lib/windows";
import { describeError } from "./errors";
import { useSettingsStore } from "./StoreProvider";
import type { SettingsStore } from "./settings";
import { onEvent } from "./tauri/invoke";

export interface PendingUpdate {
  version: string;
  notes?: string | null;
}

export type InstallPhase =
  | { state: "idle" }
  | { state: "installing"; percent: number | null }
  | { state: "restarting" }
  | { state: "error"; message: string };

export interface UpdateSnapshot {
  pending: PendingUpdate | null;
  phase: InstallPhase;
  /** The version the banner was hidden for (Later). */
  laterFor: string | null;
}

const initial: UpdateSnapshot = { pending: null, phase: { state: "idle" }, laterFor: null };
let snap: UpdateSnapshot = initial;
const subscribers = new Set<() => void>();
let started: SettingsStore | null = null;

function set(next: Partial<UpdateSnapshot>) {
  snap = { ...snap, ...next };
  subscribers.forEach((f) => f());
}

const subscribe = (f: () => void) => {
  subscribers.add(f);
  return () => subscribers.delete(f);
};

/** Asks Rust once per window, then listens. */
function start(store: SettingsStore) {
  if (started === store) return;
  started = store;
  void store
    .pendingUpdate()
    .then((p) => p && set({ pending: p }))
    .catch(() => {});
  if (!inTauri()) return;
  onEvent<PendingUpdate>("update-available", (p) => found(p));
  onEvent<{ percent: number | null; restarting?: boolean }>("update-progress", (p) =>
    set({ phase: p.restarting ? { state: "restarting" } : { state: "installing", percent: p.percent ?? null } }),
  );
  // A click on the update notification shows the banner again, even after Later.
  onEvent("show-update", () => set({ laterFor: null }));
}

/** A check found `p` (or, with null, found nothing newer). */
export function found(p: PendingUpdate | null) {
  set({ pending: p });
}

/** Later: hide the banner for this version until Talaan restarts (the toolbar chip brings it back). */
export function later() {
  set({ laterFor: snap.pending?.version ?? null, phase: snap.phase.state === "error" ? { state: "idle" } : snap.phase });
}

export function showBanner() {
  set({ laterFor: null });
}

/** Update now: download, install, restart. While recording, Rust refuses with its own words. */
export async function install(store: SettingsStore) {
  if (snap.phase.state === "installing" || snap.phase.state === "restarting") return;
  set({ phase: { state: "installing", percent: null } });
  try {
    await store.installUpdate(); // the app restarts before this returns
    set({ phase: { state: "restarting" } });
  } catch (e) {
    set({ phase: { state: "error", message: describeError(e) } });
  }
}

/** Whether the banner shows: an update is pending and Later wasn't clicked for it (or it is installing). */
export function bannerVisible(s: UpdateSnapshot): boolean {
  if (!s.pending) return false;
  return s.phase.state === "installing" || s.phase.state === "restarting" || s.laterFor !== s.pending.version;
}

/** The words for an install in progress (shared with the menu bar popover's line). Null when idle. */
export function phaseText(t: T, phase: InstallPhase): string | null {
  switch (phase.state) {
    case "installing":
      return phase.percent === null ? t("shell.updateDownloading") : t("shell.updateDownloadingPercent", { percent: phase.percent });
    case "restarting":
      return t("shell.updateRestarting");
    case "error":
      return phase.message;
    default:
      return null;
  }
}

export function useUpdate() {
  const store = useSettingsStore();
  useEffect(() => start(store), [store]);
  const s = useSyncExternalStore(subscribe, () => snap);
  return { ...s, bannerVisible: bannerVisible(s), install: () => install(store), later, showBanner, found };
}

export const updateSnapshot = () => snap;

/** Tests only. */
export function resetUpdateForTests() {
  snap = initial;
  started = null;
}
