import { useSettings } from "../../data/hooks";
import type { MessageKey } from "../../lib/i18n";
import { useT } from "../../lib/i18n/I18nProvider";
import { navigate, routes } from "../../lib/router";
import { Label, cn } from "../../ui";
import { Logo } from "../../brand/Logo";
import { SunCorner } from "../../brand/SunCorner";
import { CallAppsTab } from "./CallAppsTab";
import { ConnectionsTab } from "./ConnectionsTab";
import { GeneralTab } from "./GeneralTab";
import { ModelsTab } from "./ModelsTab";
import { PrivacyTab } from "./PrivacyTab";
import { SummaryAITab } from "./SummaryAITab";

export const SETTINGS_TABS = [
  { id: "general", label: "settings.tab_general" },
  { id: "apps", label: "settings.tab_apps" },
  { id: "models", label: "settings.tab_models" },
  { id: "ai", label: "settings.tab_ai" },
  { id: "connections", label: "settings.tab_connections" },
  { id: "privacy", label: "settings.tab_privacy" },
] as const satisfies readonly { id: string; label: MessageKey }[];

export function SettingsWindow({ tab }: { tab: string }) {
  const { settings, update } = useSettings();
  const t = useT();
  const current = SETTINGS_TABS.find((x) => x.id === tab) ?? SETTINGS_TABS[0];
  if (!settings) return null;

  const props = { settings, update };
  const onKey = (e: React.KeyboardEvent) => {
    const i = SETTINGS_TABS.findIndex((x) => x.id === current.id);
    const next = e.key === "ArrowDown" || e.key === "ArrowRight" ? i + 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? i - 1 : undefined;
    if (next !== undefined) {
      e.preventDefault();
      navigate(routes.settings(SETTINGS_TABS[(next + SETTINGS_TABS.length) % SETTINGS_TABS.length].id));
    }
  };

  return (
    <div className="flex h-full bg-paper">
      <nav aria-label={t("settings.nav_label")} className="flex w-[220px] flex-none flex-col gap-1 border-r border-line bg-rail px-3.5 py-[22px]">
        <div className="flex items-center gap-2.5 px-2 pb-[22px]">
          <Logo size={26} />
          <span className="font-display text-[17px] font-bold tracking-[-0.02em]">{t("settings.window_title")}</span>
        </div>
        <div role="tablist" aria-label={t("settings.nav_label")} aria-orientation="vertical" onKeyDown={onKey} className="flex flex-col gap-1">
          {SETTINGS_TABS.map((x) => {
            const on = x.id === current.id;
            return (
              <button
                key={x.id}
                type="button"
                role="tab"
                id={`tab-${x.id}`}
                aria-selected={on}
                aria-controls="settings-panel"
                tabIndex={on ? 0 : -1}
                onClick={() => navigate(routes.settings(x.id))}
                className={cn(
                  "flex h-9 cursor-pointer items-center rounded-control border-0 px-3 text-left font-[inherit] text-[13px] text-ink",
                  on ? "bg-surface font-bold shadow-[inset_3px_0_0_var(--t-gold),var(--t-shadow)]" : "bg-transparent font-medium hover:bg-hover",
                )}
              >
                {t(x.label)}
              </button>
            );
          })}
        </div>
        <span className="flex-1" />
        <Label className="px-2.5">{t("settings.footer", { version: __APP_VERSION__ })}</Label>
      </nav>
      <div id="settings-panel" role="tabpanel" aria-labelledby={`tab-${current.id}`} className="relative min-h-0 min-w-0 flex-1 overflow-auto">
        <SunCorner size={200} className="absolute right-0 top-0" />
        <div className="relative mx-auto max-w-[720px] px-11 pb-11 pt-10">
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
