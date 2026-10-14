import { useSettings } from "../../data/hooks";
import { navigate, routes } from "../../lib/router";
import { cn } from "../../ui";
import { CallAppsTab } from "./CallAppsTab";
import { ConnectionsTab } from "./ConnectionsTab";
import { GeneralTab } from "./GeneralTab";
import { ModelsTab } from "./ModelsTab";
import { PrivacyTab } from "./PrivacyTab";
import { SummaryAITab } from "./SummaryAITab";

export const SETTINGS_TABS = [
  { id: "general", label: "General" },
  { id: "apps", label: "Call apps" },
  { id: "models", label: "Speech models" },
  { id: "ai", label: "Summary AI" },
  { id: "connections", label: "Connections" },
  { id: "privacy", label: "Privacy" },
] as const;

export function SettingsWindow({ tab }: { tab: string }) {
  const { settings, update } = useSettings();
  const current = SETTINGS_TABS.find((t) => t.id === tab) ?? SETTINGS_TABS[0];
  if (!settings) return null;

  const props = { settings, update };
  const onKey = (e: React.KeyboardEvent) => {
    const i = SETTINGS_TABS.findIndex((t) => t.id === current.id);
    const next = e.key === "ArrowRight" ? i + 1 : e.key === "ArrowLeft" ? i - 1 : undefined;
    if (next !== undefined) {
      e.preventDefault();
      navigate(routes.settings(SETTINGS_TABS[(next + SETTINGS_TABS.length) % SETTINGS_TABS.length].id));
    }
  };

  return (
    <div className="flex h-full flex-col bg-paper">
      <div className="border-b-[1.5px] border-edge bg-rail px-4 py-2.5">
        <div role="tablist" aria-label="Settings" onKeyDown={onKey} className="flex flex-wrap justify-center gap-1">
          {SETTINGS_TABS.map((t) => {
            const on = t.id === current.id;
            return (
              <button
                key={t.id}
                type="button"
                role="tab"
                id={`tab-${t.id}`}
                aria-selected={on}
                aria-controls="settings-panel"
                tabIndex={on ? 0 : -1}
                onClick={() => navigate(routes.settings(t.id))}
                className={cn(
                  "h-[30px] cursor-pointer rounded-control border-[1.5px] px-3 font-[inherit] text-[13px] font-semibold",
                  on ? "border-strong bg-strong text-strong-fg" : "border-transparent bg-transparent text-ink hover:bg-hover",
                )}
              >
                {t.label}
              </button>
            );
          })}
        </div>
      </div>
      <div id="settings-panel" role="tabpanel" aria-labelledby={`tab-${current.id}`} className="min-h-0 flex-1 overflow-auto px-6 py-5">
        <div className="mx-auto max-w-[720px]">
          {current.id === "general" && <GeneralTab {...props} />}
          {current.id === "apps" && <CallAppsTab {...props} />}
          {current.id === "models" && <ModelsTab {...props} />}
          {current.id === "ai" && <SummaryAITab {...props} />}
          {current.id === "connections" && <ConnectionsTab {...props} />}
          {current.id === "privacy" && <PrivacyTab {...props} />}
        </div>
      </div>
    </div>
  );
}
