import type { Ms } from "../data/types";

/** 754000 -> "12:34"; 3_725_000 -> "1:02:05" */
export function formatTime(ms: Ms): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

/** 2_520_000 -> "42 min"; 7_500_000 -> "2 h 5 min" */
export function formatDuration(ms: Ms): string {
  const minutes = Math.max(1, Math.round(ms / 60_000));
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

export type DayGroup = { key: string; label: string };

/** Bucket label used in the meeting list: "today", "yesterday", then "oct 10". */
export function dayGroup(iso: string, now: Date = new Date()): DayGroup {
  const d = new Date(iso);
  const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((startOf(now) - startOf(d)) / 86_400_000);
  if (diffDays === 0) return { key: "today", label: "today" };
  if (diffDays === 1) return { key: "yesterday", label: "yesterday" };
  const label = d.toLocaleDateString("en-US", { month: "short", day: "numeric" }).toLowerCase();
  return { key: label, label };
}

export function formatClock(iso: string): string {
  return new Date(iso).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
}

export function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric" });
}
