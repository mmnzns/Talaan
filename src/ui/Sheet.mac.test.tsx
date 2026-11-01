// @vitest-environment jsdom
// Its own file: the click tracking starts when Sheet's module loads, and other tests in a shared file would have
// started it already.
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { Sheet } from "./Sheet";

afterEach(cleanup);

describe("Sheet on the Mac webview", () => {
  it("focus goes back to a clicked opener that didn't take focus (the Mac webview)", async () => {
    function MacHarness() {
      const [open, setOpen] = useState(false);
      return (
        <div>
          {/* Clicking doesn't move focus, as on the Mac: the click itself is all Sheet can go on. */}
          <button onMouseDown={(e) => e.preventDefault()} onClick={() => setOpen(true)}>
            Opener
          </button>
          <Sheet open={open} title="Confirm" onClose={() => setOpen(false)}>
            <button>Inside</button>
          </Sheet>
        </div>
      );
    }
    const user = userEvent.setup();
    render(<MacHarness />);
    await user.click(screen.getByText("Opener"));
    expect(document.activeElement).toBe(screen.getByText("Inside"));
    await user.keyboard("{Escape}");
    expect(document.activeElement).toBe(screen.getByText("Opener"));
  });
});
