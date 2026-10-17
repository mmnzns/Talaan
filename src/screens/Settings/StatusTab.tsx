import { SetupCheck } from "./SetupCheck";
import type { TabProps } from "./tabProps";

/** Settings › Status, the first tab: everything a call needs (Check setup), then Diagnostics for a bug report. */
export function StatusTab(_: TabProps) {
  return (
    <div className="flex flex-col gap-6">
      <SetupCheck />
    </div>
  );
}
