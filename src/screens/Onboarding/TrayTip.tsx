import { LogoMark } from "../../brand/Logo";
import { useSettings } from "../../data/hooks";
import { useT } from "../../lib/i18n/I18nProvider";
import { onWindows } from "../../lib/platform";
import { Button } from "../../ui";

/**
 * Windows, once, after onboarding: where Talaan's icon lives. Windows hides new tray icons under the ^ arrow, so
 * the picture shows the icon being dragged from that overflow onto the taskbar, next to the clock. Drawn with
 * plain shapes and the logo mark (no image files). A strip under the toolbar, so it never covers what's below.
 */
export function TrayTip() {
  const t = useT();
  const { settings, update } = useSettings();
  if (!onWindows || !settings?.showTrayTip) return null;
  const dismiss = () => void update({ showTrayTip: false });
  return (
    <aside aria-labelledby="tray-tip-title" className="flex flex-wrap items-center gap-x-5 gap-y-2 border-b border-line bg-surface px-6 py-3">
      <TrayArt label={t("onboarding.trayTipArt")} />
      <div className="min-w-[260px] flex-1">
        <h2 id="tray-tip-title" className="m-0 font-display text-[15px] font-bold tracking-[-0.02em]">
          {t("onboarding.trayTipTitle")}
        </h2>
        <p className="m-0 mt-0.5 max-w-[620px] text-[13px] leading-[19px] text-subtle">{t("onboarding.trayTipBody")}</p>
      </div>
      <Button variant="primary" onClick={dismiss}>
        {t("onboarding.trayTipOk")}
      </Button>
    </aside>
  );
}

/** A taskbar's right end (240 x 72): the overflow panel open above the ^ arrow with Talaan's icon in it, a dashed
 *  path to its place on the taskbar, and the clock. Positions are fixed so the path lands on the icon. */
function TrayArt({ label }: { label: string }) {
  const icon = "grid size-5 place-items-center rounded-[5px] bg-strong";
  return (
    <div role="img" aria-label={label} className="relative h-[72px] w-[240px] flex-none overflow-hidden rounded-[10px] border border-line bg-sunken">
      {/* the overflow panel above the arrow */}
      <div className="absolute left-[30px] top-[8px] flex gap-1 rounded-[7px] border border-line bg-surface p-1 shadow-card">
        <span className="size-5 rounded-[5px] bg-line" />
        <span className={`${icon} outline-2 outline-offset-1 outline-dashed outline-gold`}>
          <LogoMark size={14} className="text-[var(--t-logo-gold)]" />
        </span>
        <span className="size-5 rounded-[5px] bg-line" />
      </div>
      {/* the drag path, from the panel down to the taskbar */}
      <svg aria-hidden="true" className="absolute left-0 top-0" width="240" height="72" viewBox="0 0 240 72">
        <path d="M 84 16 C 118 2, 152 10, 157 36" fill="none" stroke="var(--t-gold)" strokeWidth="2" strokeDasharray="4 3" />
        <path d="M 152 31 L 157 38 L 162 31" fill="none" stroke="var(--t-gold)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      {/* the taskbar */}
      <div className="absolute inset-x-0 bottom-0 flex h-[30px] items-center justify-end gap-1.5 border-t border-line bg-surface px-2.5">
        <span className="grid size-5 place-items-center rounded-[5px] font-mono text-xs font-bold leading-none text-ink outline-1 outline-line">^</span>
        <span className="size-4 rounded-[4px] bg-line" />
        <span className={icon}>
          <LogoMark size={14} className="text-[var(--t-logo-gold)]" />
        </span>
        <span className="size-4 rounded-[4px] bg-line" />
        <span className="ml-1 flex w-[30px] flex-col items-end font-mono text-[9px] leading-[11px] text-subtle">
          <span>10:42</span>
          <span>16/10</span>
        </span>
      </div>
    </div>
  );
}
