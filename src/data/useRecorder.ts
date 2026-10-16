import { useEffect, useState } from "react";
import { IDLE, type RecorderClient, type RecorderLevels, type RecorderNotice, type RecorderState } from "./recorder";

/**
 * The recorder's state: asks `status()` once, then follows `recorder-state` events. An event is always newer than
 * the `status()` reply it raced (the reply can arrive after an event sent later), so once an event has arrived
 * the reply is ignored. Returns the unsubscribe.
 */
export function followRecorderState(client: RecorderClient, onState: (s: RecorderState, fromEvent: boolean) => void): () => void {
  let alive = true;
  let heard = false;
  client.status().then(
    (s) => {
      if (alive && !heard) onState(s, false);
    },
    () => {},
  );
  const off = client.onState((s) => {
    heard = true;
    if (alive) onState(s, true);
  });
  return () => {
    alive = false;
    off();
  };
}

/** Live recorder state, levels and the latest notice for one client. */
export function useRecorder(client: RecorderClient) {
  const [state, setState] = useState<RecorderState>(IDLE);
  const [levels, setLevels] = useState<RecorderLevels>({ me: 0, them: 0 });
  const [notice, setNotice] = useState<RecorderNotice>();

  useEffect(() => {
    const offs = [
      followRecorderState(client, (s, fromEvent) => {
        setState(s);
        if (fromEvent && s.state === "idle") setNotice(undefined);
      }),
      client.onLevels(setLevels),
      client.onNotice(setNotice),
    ];
    return () => offs.forEach((off) => off());
  }, [client]);

  return { state, levels, notice, clearNotice: () => setNotice(undefined) };
}
