import { phaseText, useUpdate } from "../../data/updates";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button } from "../../ui";

/**
 * "Talaan {version} is ready", a strip under the toolbar (like the tray tip, so it covers nothing). Update now
 * installs and restarts; Later hides it for this version until the next launch (the toolbar chip brings it back).
 */
export function UpdateBanner() {
  const t = useT();
  const u = useUpdate();
  if (!u.pending || !u.bannerVisible) return null;
  const busy = u.phase.state === "installing" || u.phase.state === "restarting";
  const progress = phaseText(t, u.phase);
  const percent = u.phase.state === "installing" ? u.phase.percent : u.phase.state === "restarting" ? 100 : null;
  return (
    <aside aria-labelledby="update-banner-title" className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-line bg-gold-soft px-6 py-2.5">
      <span aria-hidden="true" className="size-2 flex-none rounded-full bg-gold shadow-[0_0_0_3px_color-mix(in_srgb,var(--t-gold)_30%,transparent)]" />
      <div role="status" aria-live="polite" className="flex min-w-[220px] flex-1 flex-col">
        <h2 id="update-banner-title" className="m-0 font-display text-[14px] font-bold leading-5 tracking-[-0.02em] text-ink">
          {t("shell.updateBannerTitle", { version: u.pending.version })}
        </h2>
        <p className={`m-0 text-[13px] leading-[18px] ${u.phase.state === "error" ? "text-warn-fg" : "text-subtle"}`}>{progress ?? t("shell.updateBannerBody")}</p>
        {busy && (
          <div aria-hidden="true" className="mt-1.5 h-1 w-full max-w-[320px] overflow-hidden rounded-full bg-surface">
            <div className={`h-full rounded-full bg-gold ${percent === null ? "w-1/3 animate-pulse" : ""}`} style={percent === null ? undefined : { width: `${percent}%` }} />
          </div>
        )}
      </div>
      {!busy && (
        <div className="flex gap-2">
          <Button variant="ghost" onClick={u.later}>
            {t("shell.updateLater")}
          </Button>
          <Button variant="primary" onClick={() => void u.install()}>
            {t(u.phase.state === "error" ? "shell.updateRetry" : "shell.updateNow")}
          </Button>
        </div>
      )}
    </aside>
  );
}
