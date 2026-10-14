import { useSettings } from "../../data/hooks";
import type { ThemePreference } from "../../lib/theme";
import { navigate, routes } from "../../lib/router";
import { Button, Label } from "../../ui";
import { KitGallery } from "./KitGallery";

const SECTIONS: { id: string; label: string }[] = [{ id: "kit", label: "UI kit" }];

/** Dev-only stage: every surface that is not a normal window (menu bar, banners, prompts), plus the UI kit. */
export function ScreensStage({ which }: { which?: string }) {
  const { settings, update } = useSettings();
  const current = which ?? "kit";

  return (
    <div className="flex h-full flex-col bg-paper">
      <header className="flex flex-wrap items-center gap-3 border-b-[1.5px] border-edge bg-rail px-4 py-2.5">
        <Button onClick={() => navigate(routes.meetings())}>← Back to Talaan</Button>
        <span className="text-base font-extrabold tracking-tight">Screens</span>
        <Label>dev stage · surfaces that live outside the main window</Label>
        <span className="flex-1" />
        <Label>Theme</Label>
        {(["system", "light", "dark"] as ThemePreference[]).map((t) => (
          <Button key={t} variant={settings?.theme === t ? "strong" : "default"} onClick={() => update({ theme: t })}>
            {t}
          </Button>
        ))}
      </header>
      <div className="flex min-h-0 flex-1">
        <nav className="flex w-44 flex-col gap-0.5 border-r-[1.5px] border-edge bg-rail p-2" aria-label="Screens">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              onClick={() => navigate(routes.screens(s.id))}
              className={`cursor-pointer rounded-control border-0 px-2 py-1.5 text-left font-[inherit] text-[13px] ${
                current === s.id ? "bg-strong font-semibold text-strong-fg" : "bg-transparent text-ink hover:bg-hover"
              }`}
            >
              {s.label}
            </button>
          ))}
        </nav>
        <main className="min-w-0 flex-1 overflow-auto p-6">{current === "kit" ? <KitGallery /> : null}</main>
      </div>
    </div>
  );
}
