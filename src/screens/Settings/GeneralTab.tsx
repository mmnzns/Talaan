import { useEffect, useState } from "react";
import { Logo } from "../../brand/Logo";
import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import { openLink } from "../../lib/windows";
import { Button, Field, Label, Select, Switch } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import { SetupCheck } from "./SetupCheck";
import type { TabProps } from "./tabProps";
import { usePlatform } from "../../lib/platform";
import { useT } from "../../lib/i18n/I18nProvider";
import { LANGS, LANG_NAMES } from "../../lib/i18n";

export function GeneralTab({ settings: s, update }: TabProps) {
  const t = useT();
  const platform = usePlatform();
  return (
    <div className="flex flex-col gap-6">
    <Group>
      <Row title={t("settings.your_name")} desc={t("settings.your_name_desc")}>
        <Field
          label={t("settings.your_name")}
          placeholder={t("meeting.speakerMe")}
          defaultValue={s.yourName}
          onBlur={(e) => e.target.value.trim() !== s.yourName && update({ yourName: e.target.value.trim() })}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          wrapperClassName="w-40"
        />
      </Row>
      <Row title={t("settings.detect_title")} desc={t("settings.detect_desc")}>
        <Switch aria-label={t("settings.detect_title")} checked={s.detectCalls} onChange={(v) => update({ detectCalls: v })} />
      </Row>
      <Row title={t("settings.login_title")} desc={t("settings.login_desc", { tray: platform.trayName })}>
        <Switch aria-label={t("settings.login_title")} checked={s.launchAtLogin} onChange={(v) => update({ launchAtLogin: v })} />
      </Row>
      <Row title={t("settings.notice_title")} desc={t("settings.notice_desc")} stack>
        <textarea
          aria-label={t("settings.notice_title")}
          defaultValue={s.recordingNotice}
          onBlur={(e) => e.target.value !== s.recordingNotice && update({ recordingNotice: e.target.value })}
          rows={2}
          className="w-full resize-y rounded-control border border-input bg-paper p-2.5 font-[inherit] text-[13px] leading-[18px] text-ink outline-none focus:border-edge"
        />
      </Row>
      <MicrophoneRow settings={s} update={update} />
      <Row title={t("settings.speakers_title")} desc={t("settings.speakers_desc")}>
        <Switch aria-label={t("settings.speakers_title")} checked={s.warnOnSpeakers} onChange={(v) => update({ warnOnSpeakers: v })} />
      </Row>
      <Row title={t("settings.appearance_title")} desc={t("settings.appearance_desc")}>
        <Select label={t("settings.appearance_title")} wrapperClassName="w-40" value={s.theme} onChange={(e) => update({ theme: e.target.value as TabProps["settings"]["theme"] })}>
          <option value="system">{t("settings.theme_auto")}</option>
          <option value="light">{t("settings.theme_light")}</option>
          <option value="dark">{t("settings.theme_dark")}</option>
        </Select>
      </Row>
      <Row title={t("settings.language_title")} desc={t("settings.language_desc")}>
        <Select label={t("settings.language_title")} wrapperClassName="w-40" value={s.uiLanguage} onChange={(e) => update({ uiLanguage: e.target.value as TabProps["settings"]["uiLanguage"] })}>
          <option value="auto">{t("settings.language_auto")}</option>
          {LANGS.map((l) => (
            <option key={l} value={l} lang={l}>
              {LANG_NAMES[l]}
            </option>
          ))}
        </Select>
      </Row>
      <Row title={t("settings.notify_title")} desc={t("settings.notify_desc")}>
        <Switch aria-label={t("settings.notify_title")} checked={s.notifySummaryReady} onChange={(v) => update({ notifySummaryReady: v })} />
      </Row>
      <UpdatesRow settings={s} update={update} />
    </Group>
    <SetupCheck />
    <About />
    </div>
  );
}

type Check = { state: "idle" | "checking" | "current" | "installing" } | { state: "available"; version: string } | { state: "error"; message: string };

function UpdatesRow({ settings: s, update }: TabProps) {
  const t = useT();
  const store = useSettingsStore();
  const [check, setCheck] = useState<Check>({ state: "idle" });
  const run = async () => {
    setCheck({ state: "checking" });
    try {
      const u = await store.checkForUpdate();
      setCheck(u ? { state: "available", version: u.version } : { state: "current" });
    } catch (e) {
      setCheck({ state: "error", message: describeError(e) });
    }
  };
  const install = async () => {
    setCheck({ state: "installing" });
    try {
      await store.installUpdate(); // relaunches when done
    } catch (e) {
      setCheck({ state: "error", message: describeError(e) });
    }
  };
  const desc =
    check.state === "available"
      ? t("settings.updates_available", { version: check.version })
      : check.state === "current"
        ? t("settings.updates_current", { version: __APP_VERSION__ })
        : check.state === "error"
          ? check.message
          : check.state === "installing"
            ? t("settings.updates_installing")
            : t("settings.updates_version", { version: __APP_VERSION__ });
  return (
    <>
      <Row title={t("settings.updates_title")} desc={desc}>
        {check.state === "available" ? (
          <Button variant="primary" onClick={() => void install()}>
            {t("settings.updates_now")}
          </Button>
        ) : (
          <Button disabled={check.state === "checking" || check.state === "installing"} onClick={() => void run()}>
            {check.state === "checking" ? t("settings.updates_checking") : t("settings.updates_check")}
          </Button>
        )}
      </Row>
      <Row title={t("settings.updates_auto_title")} desc={t("settings.updates_auto_desc")}>
        <Switch aria-label={t("settings.updates_auto_title")} checked={s.checkUpdates} onChange={(v) => update({ checkUpdates: v })} />
      </Row>
    </>
  );
}

const REPO = "https://github.com/mmnzns/Talaan";
const COFFEE = "https://buymeacoffee.com/mnmnzns";

function About() {
  const t = useT();
  return (
    <div className="flex flex-col gap-3">
      <SectionIntro title={t("settings.about_title")} />
      <div className="flex items-start gap-4 rounded-card border border-line bg-surface p-[22px] shadow-card">
        <Logo size={48} />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="font-display text-xl font-bold tracking-[-0.02em]">talaan</span>
          <Label>
            {t("settings.about_version", { version: __APP_VERSION__ })}
          </Label>
          <p className="m-0 mt-1 text-[13px] leading-[19px] text-subtle">
            {t("settings.about_desc")}
          </p>
          <div className="mt-2.5 flex flex-wrap gap-2">
            <Button variant="primary" onClick={() => void openLink(COFFEE)}>
              {t("settings.about_coffee")}
            </Button>
            <Button onClick={() => void openLink(REPO)}>{t("settings.about_source")}</Button>
            <Button onClick={() => void openLink(`${REPO}/blob/main/THIRD_PARTY_NOTICES.md`)}>{t("settings.about_credits")}</Button>
          </div>
        </div>
      </div>
    </div>
  );
}

function MicrophoneRow({ settings: s, update }: TabProps) {
  const t = useT();
  const store = useSettingsStore();
  const [inputs, setInputs] = useState<string[]>([]);
  useEffect(() => {
    store.audioInputs().then(setInputs, () => setInputs([]));
  }, [store]);
  const chosen = s.microphone && s.microphone !== "default" ? s.microphone : "default";
  const missing = chosen !== "default" && inputs.length > 0 && !inputs.includes(chosen);
  return (
    <Row title={t("settings.mic_title")} desc={missing ? t("settings.mic_missing") : t("settings.mic_desc")}>
      <Select label={t("settings.mic_title")} wrapperClassName="w-56" value={chosen} onChange={(e) => update({ microphone: e.target.value })}>
        <option value="default">{t("settings.mic_default")}</option>
        {inputs.map((n) => (
          <option key={n} value={n}>
            {n}
          </option>
        ))}
        {missing && <option value={chosen}>{t("settings.mic_not_connected", { name: chosen })}</option>}
      </Select>
    </Row>
  );
}
