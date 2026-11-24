// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "./defaultSettings";
import { useMeeting, useMeetings, useSettings } from "./hooks";
import type { Settings, SettingsStore } from "./settings";
import { StoreProvider } from "./StoreProvider";
import type { MeetingStore } from "./store";

afterEach(cleanup);

function wrap(meetings: Partial<MeetingStore>, settings: Partial<SettingsStore> = {}) {
  const m = { subscribe: () => () => {}, ...meetings } as unknown as MeetingStore;
  const s = { subscribe: () => () => {}, ...settings } as unknown as SettingsStore;
  return ({ children }: { children: ReactNode }) => (
    <StoreProvider meetings={m} settings={s}>
      {children}
    </StoreProvider>
  );
}

describe("useMeeting", () => {
  it("reports an unreadable meeting instead of loading forever, and retry asks again", async () => {
    const getMeeting = vi.fn().mockRejectedValueOnce({ code: "unsupported_schema", message: "newer" }).mockResolvedValueOnce({ id: "a" });
    const { result } = renderHook(() => useMeeting("a"), { wrapper: wrap({ getMeeting }) });
    await waitFor(() => expect(result.current.error).toBeTruthy());
    expect(result.current.meeting).toBeUndefined();
    act(() => result.current.retry());
    await waitFor(() => expect(result.current.meeting).toEqual({ id: "a" }));
    expect(result.current.error).toBeUndefined();
  });
});

describe("useMeetings", () => {
  it("reports a failed list read, not an empty list", async () => {
    const listMeetings = vi.fn().mockRejectedValue(new Error("ipc"));
    const { result } = renderHook(() => useMeetings(), { wrapper: wrap({ listMeetings }) });
    await waitFor(() => expect(result.current.error).toBeTruthy());
    expect(result.current.meetings).toEqual([]);
  });

  it("keeps the last good list when a later read fails", async () => {
    const item = { id: "a" };
    const listMeetings = vi.fn().mockResolvedValueOnce([item]).mockRejectedValue(new Error("ipc"));
    const { result } = renderHook(() => useMeetings(), { wrapper: wrap({ listMeetings }) });
    await waitFor(() => expect(result.current.meetings).toEqual([item]));
    act(() => result.current.retry());
    await waitFor(() => expect(listMeetings).toHaveBeenCalledTimes(2));
    expect(result.current.error).toBeUndefined();
    expect(result.current.meetings).toEqual([item]);
  });
});

describe("useSettings", () => {
  const base: Settings = { ...defaultSettings, theme: "system" };

  it("keeps a change on screen when saving fails, says so, and retry saves it", async () => {
    const update = vi.fn().mockRejectedValueOnce({ code: "refused", message: "disk full" }).mockImplementation(async (p: Partial<Settings>) => ({ ...base, ...p }));
    const { result } = renderHook(() => useSettings(), { wrapper: wrap({}, { get: async () => base, update }) });
    await waitFor(() => expect(result.current.settings).toBeTruthy());
    await act(() => result.current.update({ theme: "dark" }));
    expect(result.current.saveError).toBeTruthy();
    expect(result.current.settings!.theme).toBe("dark");
    await act(async () => result.current.retrySave());
    await waitFor(() => expect(result.current.saveError).toBeUndefined());
    expect(update).toHaveBeenLastCalledWith({ theme: "dark" });
  });

  it("goes back to what is saved when the change is given up", async () => {
    const update = vi.fn().mockRejectedValue(new Error("x"));
    const { result } = renderHook(() => useSettings(), { wrapper: wrap({}, { get: async () => base, update }) });
    await waitFor(() => expect(result.current.settings).toBeTruthy());
    await act(() => result.current.update({ theme: "dark" }));
    act(() => result.current.dismissSaveError());
    expect(result.current.saveError).toBeUndefined();
    expect(result.current.settings!.theme).toBe("system");
  });

  it("shows no error when saving works", async () => {
    const update = vi.fn(async (p: Partial<Settings>) => ({ ...base, ...p }));
    const { result } = renderHook(() => useSettings(), { wrapper: wrap({}, { get: async () => base, update }) });
    await waitFor(() => expect(result.current.settings).toBeTruthy());
    await act(() => result.current.update({ theme: "dark" }));
    expect(result.current.saveError).toBeUndefined();
  });
});
