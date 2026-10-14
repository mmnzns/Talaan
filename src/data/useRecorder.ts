import { useEffect, useState } from "react";
import { IDLE, type RecorderClient, type RecorderLevels, type RecorderNotice, type RecorderState } from "./recorder";

/** Live recorder state, levels and the latest notice for one client. */
export function useRecorder(client: RecorderClient) {
  const [state, setState] = useState<RecorderState>(IDLE);
  const [levels, setLevels] = useState<RecorderLevels>({ me: 0, them: 0 });
  const [notice, setNotice] = useState<RecorderNotice>();

  useEffect(() => {
    let alive = true;
    client.status().then((s) => alive && setState(s), () => {});
    const offs = [
      client.onState((s) => {
        setState(s);
        if (s.state === "idle") setNotice(undefined);
      }),
      client.onLevels(setLevels),
      client.onNotice(setNotice),
    ];
    return () => {
      alive = false;
      offs.forEach((off) => off());
    };
  }, [client]);

  return { state, levels, notice, clearNotice: () => setNotice(undefined) };
}
