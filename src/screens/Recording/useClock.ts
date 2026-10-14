import { useEffect, useState } from "react";

/** Counts up once a second while `running`. Returns elapsed milliseconds. */
export function useElapsed(startMs: number, running: boolean): number {
  const [ms, setMs] = useState(startMs);
  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => setMs((v) => v + 1000), 1000);
    return () => clearInterval(timer);
  }, [running]);
  return ms;
}

/** Counts down from `seconds` while `running`. Calls `onZero` once when it hits 0. */
export function useCountdown(seconds: number, running: boolean, onZero: () => void) {
  const [left, setLeft] = useState(seconds);
  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => setLeft((v) => Math.max(0, v - 1)), 1000);
    return () => clearInterval(timer);
  }, [running]);
  useEffect(() => {
    if (left === 0 && running) onZero();
  }, [left, running]);
  return { left, reset: () => setLeft(seconds) };
}
