import { useCallback, useEffect, useState } from "react";
import { recorderClient } from "../../data";
import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import type { MessageKey } from "../../lib/i18n";
import { useLang, useT } from "../../lib/i18n/I18nProvider";
import { navigate, routes } from "../../lib/router";
import { checkSetup, probeSetup, type CheckResult, type CheckStatus, type FixAction } from "../../lib/setupCheck";
import { Button, Label, cn, useToast } from "../../ui";
import { Group, SectionIntro } from "./parts";

const STATUS: Record<CheckStatus, { label: MessageKey; className: string }> = {
  ok: { label: "settings.setup_status_ok", className: "border-transparent bg-gold-soft text-gold-ink" },
  warn: { label: "settings.setup_status_warn", className: "border-warn-fg text-warn-fg" },
  fail: { label: "settings.setup_status_fail", className: "border-transparent bg-warn-bg text-warn-fg" },
  unknown: { label: "settings.setup_status_unknown", className: "border-line text-subtle" },
};

/** "Check setup": everything a call needs, in one place, each with the button that fixes it. */
export function SetupCheck() {
  const t = useT();
  const lang = useLang();
  const store = useSettingsStore();
  const toast = useToast();
  const [results, setResults] = useState<CheckResult[]>();
  const [busy, setBusy] = useState(false);

  const run = useCallback(
    async (testSystemAudio = false) => {
      setBusy(true);
      try {
        setResults(checkSetup(await probeSetup(store, recorderClient, testSystemAudio)));
      } catch (e) {
        toast(describeError(e));
      } finally {
        setBusy(false);
      }
    },
    [store, toast],
  );

  // Runs again when the language changes, so the details read in it.
  useEffect(() => {
    void run();
  }, [run, lang]);

  const fix = async (action: FixAction) => {
    try {
      if (action === "allowMic") await recorderClient.requestMicrophone();
      if (action === "micSettings") await recorderClient.openSettings("microphone");
      if (action === "systemAudioSettings") await recorderClient.openSettings("system-audio");
      if (action === "speechModels") return navigate(routes.settings("models"));
      if (action === "summaryAI") return navigate(routes.settings("ai"));
      if (action === "openData") return await store.revealData();
      await run(action === "testSystemAudio");
    } catch (e) {
      toast(describeError(e));
    }
  };

  const problems = results?.filter((r) => r.status === "fail" || r.status === "warn").length ?? 0;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <SectionIntro title={t("settings.setup_title")} desc={t("settings.setup_desc")} />
        <Button disabled={busy} onClick={() => void run()}>
          {busy ? t("settings.setup_checking") : t("settings.setup_again")}
        </Button>
      </div>
      <Group>
        {results?.map((r) => (
          <div key={r.id} className="flex flex-wrap items-center gap-x-4 gap-y-2 py-3.5">
            <span className={cn("inline-flex min-w-[72px] flex-none justify-center rounded-full border px-2 py-[3px] font-mono text-[11px] leading-[14px]", STATUS[r.status].className)}>
              {t(STATUS[r.status].label)}
            </span>
            <div className="min-w-0 flex-1 basis-56">
              <div className="font-semibold">{r.title}</div>
              <div className="mt-0.5 text-xs leading-[17px] text-subtle">{r.detail}</div>
            </div>
            {r.fix && (
              <Button variant={r.status === "fail" ? "primary" : "default"} disabled={busy} onClick={() => void fix(r.fix!.action)}>
                {r.fix.label}
              </Button>
            )}
          </div>
        )) ?? <div className="py-4 text-[13px] text-subtle">{t("settings.setup_checking")}</div>}
      </Group>
      {results && <Label>{problems === 0 ? t("settings.setup_none") : t("settings.setup_problems", { count: problems })}</Label>}
    </div>
  );
}
