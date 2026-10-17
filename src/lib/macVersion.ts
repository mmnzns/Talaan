import { useEffect, useState } from "react";
import { call } from "../data/tauri/invoke";
import { onMac } from "./platform";
import { inTauri } from "./windows";

/** macOS's major version (14, 15, 26…), asked once from Rust (`system_macos_major`); undefined until known, or on
 *  Windows. The browser preview takes `?macos=14`. */
export function useMacMajor(): number | undefined {
  const [major, setMajor] = useState<number>();
  useEffect(() => {
    if (!onMac) return;
    if (!inTauri()) {
      const v = Number(new URLSearchParams(window.location.search).get("macos"));
      if (v > 0) setMajor(v);
      return;
    }
    let live = true;
    call<number | null>("system_macos_major").then(
      (v) => live && v != null && setMajor(v),
      () => {},
    );
    return () => {
      live = false;
    };
  }, []);
  return major;
}
