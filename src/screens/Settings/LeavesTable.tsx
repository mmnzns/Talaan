import type { Settings } from "../../data/settings";
import type { MessageKey } from "../../lib/i18n";
import { useT } from "../../lib/i18n/I18nProvider";
import { onWindows } from "../../lib/platform";
import { Label, cn } from "../../ui";

/** What leaves this computer, and when. One list for Settings › Privacy and onboarding's summary step. */
export function privacyFlows(s: Settings, windows = onWindows): [MessageKey, MessageKey][] {
  return [
    ["settings.flow_audio", "settings.when_never"],
    ["settings.flow_transcripts", "settings.when_never"],
    ["settings.flow_byok", s.summaryEngine === "byok" ? "settings.when_on" : "settings.when_off"],
    ["settings.flow_notion", s.notion.connected ? "settings.when_export" : "settings.when_not_connected"],
    ["settings.flow_update", s.checkUpdates ? "settings.when_launch" : "settings.when_off"],
    ...(windows && s.calendarConnected ? [["settings.flow_calendar_windows", "settings.when_recording_start"] as [MessageKey, MessageKey]] : []),
    ["settings.flow_analytics", "settings.when_never_collected"],
  ];
}

export function LeavesTable({ settings, compact }: { settings: Settings; compact?: boolean }) {
  const t = useT();
  return (
    <div className={cn("flex flex-col", compact ? "gap-1.5" : "gap-2.5 py-4")}>
      <Label>{t("settings.leaves_label")}</Label>
      <div className={cn("grid grid-cols-[1fr_auto] gap-x-4", compact ? "gap-y-1 text-xs leading-[17px]" : "gap-y-1.5 text-[13px]")}>
        {privacyFlows(settings).map(([what, when]) => (
          <div key={what} className="contents">
            <span>{t(what)}</span>
            <span className={when === "settings.when_on" ? "font-bold text-danger" : "font-mono text-xs text-subtle"}>{t(when)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
