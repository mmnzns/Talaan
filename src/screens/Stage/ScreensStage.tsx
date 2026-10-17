import { useSettings } from "../../data/hooks";
import { usingRealStorage } from "../../data";
import type { ThemePreference } from "../../lib/theme";
import { navigate, routes } from "../../lib/router";
import { Button, Label } from "../../ui";
import { DevDataStage } from "./DevDataStage";
import { KitGallery } from "./KitGallery";
import { MenuBarStage, PromptsStage, RecordingStage } from "./SurfaceStages";

const SECTIONS: { id: string; label: string }[] = [
  { id: "onboarding", label: "Onboarding" },
  { id: "menubar", label: "Menu bar" },
  { id: "recording", label: "Recording" },
  { id: "prompts", label: "Prompts" },
  { id: "kit", label: "UI kit" },
  { id: "devdata", label: "Dev data" },
];

/** Dev-only stage: every surface that is not a normal window (menu bar, banners, prompts), plus the UI kit. */
export function ScreensStage({ which }: { which?: string }) {
  const { settings, update } = useSettings();
  const current = which ?? "onboarding";

  return (
    <div className="flex h-full flex-col bg-paper">
      <header className="flex flex-wrap items-center gap-3 border-b border-edge bg-rail px-4 py-2.5">
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
        <nav className="flex w-44 flex-col gap-0.5 border-r border-edge bg-rail p-2" aria-label="Screens">
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
        <main className="min-w-0 flex-1 overflow-auto p-6">
          {current === "kit" && <KitGallery />}
          {current === "devdata" && <DevDataStage />}
          {current === "menubar" && <MenuBarStage />}
          {current === "recording" && <RecordingStage />}
          {current === "prompts" && <PromptsStage />}
          {current === "onboarding" && (
            <div className="flex max-w-md flex-col gap-3">
              <h2 className="m-0 text-lg font-extrabold">Onboarding</h2>
              <p className="m-0 text-[13px] text-subtle">
                Shows the first-run flow again in the main window. Finishing it brings you back to Talaan. In the browser preview, "as a new install" also
                marks every model as not downloaded. Add ?os=mac or ?os=windows, and ?mic=denied or ?mic=ask, to the URL to see those cases.
              </p>
              <div className="flex gap-2">
                <Button
                  variant="primary"
                  onClick={async () => {
                    await update({ onboardingDone: false });
                    navigate(routes.meetings());
                  }}
                >
                  Restart onboarding
                </Button>
                {!usingRealStorage && settings && (
                  <Button
                    onClick={async () => {
                      const none = <T extends { state: string; progress?: number }>(m: T): T => ({ ...m, state: "not_downloaded", progress: undefined, partial: undefined, error: undefined, cancelled: undefined });
                      await update({
                        onboardingDone: false,
                        showTrayTip: false,
                        testMeetingId: undefined,
                        yourName: "",
                        speechModels: settings.speechModels.map(none),
                        localModels: settings.localModels.map(none),
                      });
                      navigate(routes.meetings());
                    }}
                  >
                    Restart as a new install
                  </Button>
                )}
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
