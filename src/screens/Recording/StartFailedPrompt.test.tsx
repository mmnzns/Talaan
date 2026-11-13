// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StartFailedPrompt } from "./CallPrompts";

afterEach(cleanup);

describe("StartFailedPrompt", () => {
  it("says what Rust says about the failed start, and can be dismissed", async () => {
    const onDismiss = vi.fn();
    render(<StartFailedPrompt app="Zoom" message="Talaan couldn't start recording Zoom: No microphone found. Press Record to try again." onDismiss={onDismiss} />);
    expect(screen.getByRole("alertdialog", { name: "Couldn't start recording" })).toBeTruthy();
    expect(screen.getByText(/No microphone found\. Press Record to try again\./)).toBeTruthy();
    await userEvent.setup().click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onDismiss).toHaveBeenCalled();
  });
});
