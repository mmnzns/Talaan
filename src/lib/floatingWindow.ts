import { useEffect, type RefObject } from "react";
import { inTauri } from "./windows";

/** Room around the content for the hard offset shadow. */
const PAD = 8;

/**
 * Sizes the current (frameless, transparent) window to its content. With `corner: "top-right"` it also pins the
 * window to the top-right of the screen, under the menu bar (the call prompts); otherwise the position is left
 * to Rust (the menu bar popover sits under its icon).
 */
export function useFitWindow(ref: RefObject<HTMLElement | null>, corner?: "top-right") {
  useEffect(() => {
    const el = ref.current;
    if (!el || !inTauri()) return;
    let live = true;
    const fit = async () => {
      const { getCurrentWindow, currentMonitor, LogicalSize, LogicalPosition } = await import("@tauri-apps/api/window");
      if (!live) return;
      const w = Math.ceil(el.offsetWidth) + PAD;
      const h = Math.ceil(el.offsetHeight) + PAD;
      const win = getCurrentWindow();
      await win.setSize(new LogicalSize(w, h));
      if (corner === "top-right") {
        const m = await currentMonitor();
        if (m) await win.setPosition(new LogicalPosition(m.size.width / m.scaleFactor - w - 12, 34));
      }
    };
    const ro = new ResizeObserver(() => void fit());
    ro.observe(el);
    void fit();
    return () => {
      live = false;
      ro.disconnect();
    };
  }, [ref, corner]);
}

/** Frameless windows show only their card: no page background. */
export function useTransparentPage() {
  useEffect(() => {
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
  }, []);
}
