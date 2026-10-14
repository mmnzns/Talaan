import { Download, MessageCircle, Settings as SettingsIcon } from "lucide-react";
import { useSettings } from "../../data/hooks";
import { Logo } from "../../brand/Logo";
import { inTauri, openSettings } from "../../lib/windows";
import { navigate, routes } from "../../lib/router";
import { Button, Chip, cn } from "../../ui";

interface Props {
  onImport: () => void;
}

export function Toolbar({ onImport }: Props) {
  const { settings } = useSettings();
  const local = !settings || settings.summaryEngine === "local" || settings.summaryEngine === "ollama" || settings.summaryEngine === "lmstudio";

  return (
    <header
      data-tauri-drag-region
      className={cn(
        "flex flex-wrap items-center gap-2.5 border-b-[1.5px] border-edge bg-rail px-4 py-2.5",
        inTauri() && "pl-[84px]",
      )}
    >
      <Logo />
      <span className="text-base font-extrabold tracking-[-0.03em]">Talaan</span>
      <Chip>
        <span className="size-[7px] rounded-full bg-lime shadow-[0_0_0_3px_color-mix(in_srgb,var(--t-lime)_30%,transparent)]" />
        {local ? "Offline · Local AI" : "Cloud summaries on"}
      </Chip>
      <span data-tauri-drag-region className="flex-1 self-stretch" />
      <Button onClick={() => navigate(routes.ask())}>
        <MessageCircle size={15} aria-hidden="true" />
        Ask
      </Button>
      <Button onClick={onImport}>
        <Download size={15} aria-hidden="true" className="rotate-180" />
        Import audio
      </Button>
      <Button variant="primary" onClick={() => navigate(routes.screens("recording"))}>
        <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24">
          <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="2" />
          <circle cx="12" cy="12" r="4" fill="currentColor" />
        </svg>
        Record
      </Button>
      <Button size="icon" aria-label="Settings" onClick={() => openSettings()}>
        <SettingsIcon size={16} aria-hidden="true" />
      </Button>
    </header>
  );
}
