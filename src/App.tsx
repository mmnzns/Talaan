import { useEffect } from "react";
import { useSettings } from "./data/hooks";
import { applyTheme } from "./lib/theme";
import { useRoute } from "./lib/router";
import { MainWindow } from "./screens/Shell/MainWindow";
import { OnboardingScreen } from "./screens/Onboarding/OnboardingScreen";
import { SettingsWindow } from "./screens/Settings/SettingsWindow";
import { ScreensStage } from "./screens/Stage/ScreensStage";
import { PromptWindow } from "./screens/MenuBar/PromptWindow";
import { TrayWindow } from "./screens/MenuBar/TrayWindow";
import { PrintView } from "./screens/Meeting/PrintView";

export default function App() {
  const route = useRoute();
  const { settings } = useSettings();
  const theme = settings?.theme;

  useEffect(() => (theme ? applyTheme(theme) : undefined), [theme]);

  if (!settings) return null;
  if (route.name === "settings") return <SettingsWindow tab={route.tab} />;
  if (route.name === "screens") return <ScreensStage which={route.which} />;
  // The menu bar popover and the call prompts are their own small windows.
  if (route.name === "tray") return <TrayWindow />;
  if (route.name === "prompt") return <PromptWindow />;
  if (route.name === "print") return <PrintView id={route.id} withTranscript={route.withTranscript} />;
  if (!settings.onboardingDone) return <OnboardingScreen />;
  return <MainWindow route={route} />;
}
