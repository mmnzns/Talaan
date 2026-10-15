import { Download, MessageCircle, Settings as SettingsIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useSettings } from "../../data/hooks";
import { onEvent } from "../../data/tauri/invoke";
import type { RecorderState } from "../../data/recorder";
import { formatTime } from "../../lib/format";
import { onMac } from "../../lib/os";
import { Logo } from "../../brand/Logo";
import { inTauri, openSettings } from "../../lib/windows";
import { navigate, routes } from "../../lib/router";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Chip, cn } from "../../ui";

interface Props {
  onImport: () => void;
  onRecord: () => void;
  recording: RecorderState;
}

export function Toolbar({ onImport, onRecord, recording }: Props) {
  const t = useT();
  const { settings } = useSettings();
  // A newer version found by the launch check (the app only; nothing is downloaded until Update now).
  const [update, setUpdate] = useState<string>();
  useEffect(() => (inTauri() ? onEvent<{ version: string }>("update-available", (u) => setUpdate(u.version)) : undefined), []);
  const local = !settings || settings.summaryEngine !== "byok";

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
      <Chip>
        <span className="size-[7px] rounded-full bg-gold shadow-[0_0_0_3px_color-mix(in_srgb,var(--t-gold)_30%,transparent)]" />
        {local ? t("shell.offlineLocal") : t("shell.cloudOn")}
      </Chip>
      {update && (
        <Chip tone="gold" onClick={() => void openSettings("general")} title={t("shell.updateReadyTitle", { version: update })}>
          {t("shell.updateReady")}
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
      <Button size="icon" aria-label={t("shell.settings")} onClick={() => openSettings()}>
        <SettingsIcon size={16} aria-hidden="true" />
      </Button>
    </header>
  );
}
