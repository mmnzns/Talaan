import { useEffect, useRef, useState } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms } from "../../data/types";

/**
 * The meeting's audio. Plays the mixed playback file through an <audio> element (made in Rust on first play);
 * where there is none (the browser preview) a simulated playhead moves instead, so the screen behaves the same.
 * `at` is the playhead in ms, on the same clock as the transcript.
 */
export function usePlayer(meeting: Meeting, onTime: (at: Ms) => void) {
  const store = useStore();
  const audio = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [simulated, setSimulated] = useState(false);
  const pos = useRef<Ms>(0);
  const report = useRef(onTime);
  report.current = onTime;

  // A different meeting: stop and forget the old audio.
  useEffect(() => {
    return () => {
      audio.current?.pause();
      audio.current = null;
      setPlaying(false);
      setSimulated(false);
    };
  }, [meeting.id]);

  // Simulated playhead (no audio file).
  useEffect(() => {
    if (!playing || !simulated) return;
    const timer = setInterval(() => {
      pos.current = Math.min(meeting.durationMs, pos.current + 500);
      report.current(pos.current);
      if (pos.current >= meeting.durationMs) setPlaying(false);
    }, 500);
    return () => clearInterval(timer);
  }, [playing, simulated, meeting.durationMs]);

  const load = async (): Promise<HTMLAudioElement | null> => {
    if (audio.current) return audio.current;
    const url = await store.playbackUrl(meeting.id).catch(() => undefined);
    if (!url) {
      setSimulated(true);
      return null;
    }
    const a = new Audio(url);
    a.preload = "auto";
    a.ontimeupdate = () => {
      pos.current = a.currentTime * 1000;
      report.current(pos.current);
    };
    a.onended = () => setPlaying(false);
    a.onpause = () => setPlaying(false);
    a.onplay = () => setPlaying(true);
    audio.current = a;
    return a;
  };

  const playFrom = async (at: Ms) => {
    pos.current = at;
    const a = await load();
    if (!a) return setPlaying(true);
    a.currentTime = at / 1000;
    await a.play().catch(() => setPlaying(false));
  };

  const toggle = async (from: Ms) => {
    if (playing) {
      audio.current?.pause();
      setPlaying(false);
      return;
    }
    await playFrom(from >= meeting.durationMs ? 0 : from);
  };

  return { playing, toggle, playFrom, canPlay: meeting.hasAudio };
}
