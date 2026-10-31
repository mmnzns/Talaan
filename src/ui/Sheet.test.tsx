// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Sheet } from "./Sheet";

afterEach(cleanup);

function Harness({ onClose = () => {}, autofocusSecond = false }: { onClose?: () => void; autofocusSecond?: boolean }) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button onClick={() => setOpen(true)}>Open</button>
      <button>Other</button>
      <Sheet
        open={open}
        title="Confirm"
        onClose={() => {
          onClose();
          setOpen(false);
        }}
      >
        <button>First</button>
        <button data-autofocus={autofocusSecond ? "" : undefined}>Second</button>
        <button disabled>Disabled</button>
        <button>Last</button>
      </Sheet>
    </div>
  );
}

describe("Sheet focus", () => {
  it("focuses the first control when it opens", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByText("Open"));
    expect(document.activeElement).toBe(screen.getByText("First"));
  });

  it("prefers the control marked data-autofocus", async () => {
    const user = userEvent.setup();
    render(<Harness autofocusSecond />);
    await user.click(screen.getByText("Open"));
    expect(document.activeElement).toBe(screen.getByText("Second"));
  });

  it("falls back to the dialog when it has no controls", async () => {
    const user = userEvent.setup();
    render(
      <Sheet open title="Empty" onClose={() => {}}>
        <p>Nothing to press</p>
      </Sheet>,
    );
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
  });

  it("keeps Tab inside the dialog, skipping disabled controls", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByText("Open"));
    await user.tab();
    expect(document.activeElement).toBe(screen.getByText("Second"));
    await user.tab();
    expect(document.activeElement).toBe(screen.getByText("Last"));
    await user.tab();
    expect(document.activeElement).toBe(screen.getByText("First"));
  });

  it("Shift+Tab wraps backwards from the first control", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByText("Open"));
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(screen.getByText("Last"));
  });

  it("brings focus back in when it was lost to the page", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByText("Open"));
    (document.activeElement as HTMLElement).blur();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByText("First"));
  });

  it("returns focus to the opener on Escape", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);
    const opener = screen.getByText("Open");
    await user.click(opener);
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("returns focus to the opener when a button inside closes it", async () => {
    const user = userEvent.setup();
    function Closing() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button onClick={() => setOpen(true)}>Open</button>
          <Sheet open={open} title="Confirm" onClose={() => setOpen(false)}>
            <button onClick={() => setOpen(false)}>Cancel</button>
          </Sheet>
        </>
      );
    }
    render(<Closing />);
    const opener = screen.getByText("Open");
    await user.click(opener);
    await user.click(screen.getByText("Cancel"));
    expect(document.activeElement).toBe(opener);
  });

  it("does not steal focus when the parent re-renders with a new onClose", async () => {
    const user = userEvent.setup();
    function Parent() {
      const [n, setN] = useState(0);
      return (
        <Sheet open title="Count" onClose={() => setN(n)}>
          <button>First</button>
          <button onClick={() => setN(n + 1)}>Bump {n}</button>
        </Sheet>
      );
    }
    render(<Parent />);
    await user.tab();
    const bump = screen.getByText("Bump 0");
    expect(document.activeElement).toBe(bump);
    await user.click(bump);
    expect(document.activeElement).toBe(screen.getByText("Bump 1"));
  });

  it("only the top-most of two stacked sheets traps Tab", async () => {
    const user = userEvent.setup();
    render(
      <>
        <Sheet open title="Under" onClose={() => {}}>
          <button>Under A</button>
        </Sheet>
        <Sheet open title="Over" onClose={() => {}}>
          <button>Over A</button>
          <button>Over B</button>
        </Sheet>
      </>,
    );
    expect(document.activeElement).toBe(screen.getByText("Over A"));
    await user.tab();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByText("Over A"));
  });

  it("Escape closes only the top-most of two stacked sheets", async () => {
    const user = userEvent.setup();
    const under = vi.fn();
    const over = vi.fn();
    render(
      <>
        <Sheet open title="Under" onClose={under}>
          <button>Under A</button>
        </Sheet>
        <Sheet open title="Over" onClose={over}>
          <button>Over A</button>
        </Sheet>
      </>,
    );
    await user.keyboard("{Escape}");
    expect(over).toHaveBeenCalledTimes(1);
    expect(under).not.toHaveBeenCalled();
  });
});
