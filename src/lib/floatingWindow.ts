import { useEffect, type RefObject } from "react";
import { inTauri } from "./windows";

/** Room around the content for the hard offset shadow. */
export const FIT_PAD = 8;
const PAD = FIT_PAD;

/**
 * Sizes the current (frameless, transparent) window to its content. Rust places it: the call prompt top right
 * under the menu bar, the menu bar popover under its icon.
 */
export function useFitWindow(ref: RefObject<HTMLElement | null>, content: unknown) {
  useEffect(() => {
    const el = ref.current;
    if (!el || !inTauri()) return;
    let live = true;
    const fit = async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      if (!live) return;
      await invoke("window_fit", { width: Math.ceil(el.offsetWidth) + PAD, height: Math.ceil(el.offsetHeight) + PAD });
    };
    const ro = new ResizeObserver(() => void fit());
    ro.observe(el);
    void fit();
    return () => {
      live = false;
      ro.disconnect();
    };
    // `content` changes when the window shows something else (a different prompt, idle vs recording), so the
    // observer is attached to the element that is actually there.
  }, [ref, content]);
}

/** Frameless windows show only their card: no page background. */
export function useTransparentPage() {
  useEffect(() => {
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
  }, []);
}
