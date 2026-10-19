import { ArrowUp, Download, MessageCircle, Settings as SettingsIcon } from "lucide-react";
import { useSettings } from "../../data/hooks";
import { UPDATE_CHIP_ID, useUpdate } from "../../data/updates";
import type { RecorderState } from "../../data/recorder";
import { formatTime } from "../../lib/format";
import { onMac } from "../../lib/platform";
import { Logo } from "../../brand/Logo";
import { inTauri, openSettings } from "../../lib/windows";
import { navigate, routes } from "../../lib/router";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Chip, cn } from "../../ui";
import { useSetupProblems } from "./useSetupProblems";
import { privacyChip } from "../../lib/privacyChip";

interface Props {
  onImport: () => void;
  onRecord: () => void;
  recording: RecorderState;
}

export function Toolbar({ onImport, onRecord, recording }: Props) {
  const t = useT();
  const { settings } = useSettings();
  // A newer version (nothing is downloaded until Update now). The banner under the toolbar says it; after
  // Later, this chip stays and brings the banner back.
  const update = useUpdate();
  const chip = update.pending && !update.bannerVisible ? update.pending.version : undefined;
  // Something in Settings › Status stops a call: a dot on the button, which then opens Status.
  const problems = useSetupProblems();

  return (
    <header
      data-tauri-drag-region
      className={cn(
        "flex flex-wrap items-center gap-3 border-b border-line bg-surface px-6 py-3.5",
        inTauri() && onMac && "pl-[84px]",
      )}
    >
      <Logo size={28} />
      <span className="font-display text-lg font-bold tracking-[-0.02em]">talaan</span>
      {/* The privacy promise in plain words; opens the Privacy tab's "what leaves this computer". */}
      <Chip className="font-sans text-xs normal-case" onClick={() => void openSettings("privacy")} title={t("shell.privacyChipTitle")}>
        <span aria-hidden="true" className="size-[7px] rounded-full bg-gold shadow-[0_0_0_3px_color-mix(in_srgb,var(--t-gold)_30%,transparent)]" />
        {t(privacyChip(settings))}
      </Chip>
      {chip && (
        <Chip id={UPDATE_CHIP_ID} tone="gold" onClick={update.showBanner} title={t("shell.updateReadyTitle", { version: chip })} aria-label={`${t("shell.updateReadyTitle", { version: chip })}. ${t("shell.updateShowBanner")}`}>
          <ArrowUp size={12} aria-hidden="true" />
          {/* Narrow windows: the arrow alone, so the toolbar stays on one line. */}
          <span className="max-lg:hidden">{t("shell.updateReady")}</span>
        </Chip>
      )}
      <span data-tauri-drag-region className="flex-1 self-stretch" />
      <Button onClick={() => navigate(routes.ask())}>
        <MessageCircle size={15} aria-hidden="true" />
        {t("shell.ask")}
      </Button>
      <Button onClick={onImport}>
        <Download size={15} aria-hidden="true" className="rotate-180" />
        {t("shell.importAudio")}
      </Button>
      {recording.state === "idle" ? (
        <Button variant="primary" onClick={onRecord}>
          <span aria-hidden="true" className="size-[9px] rounded-full bg-red shadow-[0_0_0_3px_color-mix(in_srgb,var(--t-red)_25%,transparent)]" />
          {t("shell.record")}
        </Button>
      ) : (
        <Button variant="strong" onClick={onRecord} aria-label={t("shell.showRecording")}>
          <span className={recording.state === "paused" ? "size-[9px] rounded-full bg-faint" : "size-[9px] animate-pulse rounded-full bg-red"} />
          <span className="font-mono">{recording.state === "paused" ? t("shell.paused") : formatTime(recording.elapsedMs)}</span>
        </Button>
      )}
      <Button
        size="icon"
        className="relative"
        aria-label={problems > 0 ? t("shell.settingsNeedsFix") : t("shell.settings")}
        title={problems > 0 ? t("shell.settingsNeedsFix") : undefined}
        onClick={() => void openSettings(problems > 0 ? "status" : undefined)}
      >
        <SettingsIcon size={16} aria-hidden="true" />
        {problems > 0 && <span aria-hidden="true" className="absolute right-1 top-1 size-2 rounded-full bg-danger ring-2 ring-surface" />}
      </Button>
    </header>
  );
}
