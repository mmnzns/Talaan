import { describe, expect, it } from "vitest";
import { IDLE, MockRecorderClient, type RecorderClient, type RecorderState } from "./recorder";
import { followRecorderState } from "./useRecorder";

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => (resolve = res));
  return { promise, resolve };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

/** A client whose `status()` answers only when the test says so, and whose events the test sends. */
function racingClient() {
  const reply = deferred<RecorderState>();
  let send: (s: RecorderState) => void = () => {};
  const client: RecorderClient = new MockRecorderClient();
  client.status = () => reply.promise;
  client.onState = (cb) => {
    send = cb;
    return () => {
      send = () => {};
    };
  };
  return { client, reply, send: (s: RecorderState) => send(s) };
}

const recording: RecorderState = { ...IDLE, state: "recording", elapsedMs: 5000 };

describe("followRecorderState", () => {
  it("ignores a status() reply that arrives after a newer event", async () => {
    const { client, reply, send } = racingClient();
    const seen: string[] = [];
    followRecorderState(client, (s, fromEvent) => seen.push(`${s.state}:${fromEvent}`));
    send(recording); // Start pressed in another window
    reply.resolve(IDLE); // the reply to the earlier status() request, from before Start
    await flush();
    expect(seen).toEqual(["recording:true"]);
  });

  it("uses the status() reply when no event came first, then follows events", async () => {
    const { client, reply, send } = racingClient();
    const seen: string[] = [];
    followRecorderState(client, (s, fromEvent) => seen.push(`${s.state}:${fromEvent}`));
    reply.resolve(recording);
    await flush();
    send(IDLE);
    expect(seen).toEqual(["recording:false", "idle:true"]);
  });

  it("stops listening and ignores the reply after unsubscribing", async () => {
    const { client, reply, send } = racingClient();
    const seen: string[] = [];
    const off = followRecorderState(client, (s) => seen.push(s.state));
    off();
    reply.resolve(recording);
    send(recording);
    await flush();
    expect(seen).toEqual([]);
  });
});
