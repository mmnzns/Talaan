import { useEffect, useState } from "react";
import { recorderClient } from "../../data";
import type { BrowserState, CalendarAccess, NotionDestination } from "../../data/settings";
import { describeError } from "../../data/errors";
import { openLink } from "../../lib/windows";
import { useSettingsStore } from "../../data/StoreProvider";
import { Button, Chip, Field, Select, Switch, useToast } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";
import { usePlatform } from "../../lib/platform";
import { useT } from "../../lib/i18n/I18nProvider";
import type { MessageKey } from "../../lib/i18n";

const STATE_LABEL = {
  allowed: "settings.state_allowed",
  denied: "settings.state_denied",
  not_asked: "settings.state_not_asked",
  not_installed: "settings.state_not_installed",
} as const satisfies Record<BrowserState, MessageKey>;

export function ConnectionsTab({ settings: s, update }: TabProps) {
  const toast = useToast();
  const t = useT();
  const platform = usePlatform();
  const store = useSettingsStore();
  const [calendar, setCalendar] = useState<CalendarAccess>();
  const [browsers, setBrowsers] = useState<{ name: string; state: BrowserState }[]>([]);
  useEffect(() => {
    store.calendarAccess().then(setCalendar, () => setCalendar(undefined));
    store.browsersStatus().then(setBrowsers, () => setBrowsers([]));
  }, [store]);

  const toggleCalendar = async (on: boolean) => {
    if (!on) return update({ calendarConnected: false });
    const access = await store.requestCalendar();
    setCalendar(access);
    if (access === "allowed") update({ calendarConnected: true });
    else toast(t("settings.calendar_off", { app: platform.settingsApp }));
  };

  return (
    <div className="flex flex-col gap-3">
      <SectionIntro title={t("settings.conn_title")} desc={t("settings.conn_desc")} />
      <Group>
        <Row title={t("settings.calendar_title")} desc={t("settings.calendar_desc")}>
          <span className="flex items-center gap-3">
            {calendar === "denied" && (
              <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => recorderClient.openSettings("calendar")}>
                {t("settings.allow_in", { app: platform.settingsApp })}
              </button>
            )}
            <Switch aria-label={t("settings.calendar_title")} checked={s.calendarConnected && calendar !== "denied"} onChange={toggleCalendar} />
          </span>
        </Row>
        <Row title={t("settings.browser_title")} desc={t("settings.browser_desc")}>
          <Switch aria-label={t("settings.browser_title")} checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />
          {s.tabDetection && (
            <div className="flex basis-full flex-wrap gap-2">
              {browsers.map((b) => (
                <Chip key={b.name} tone={b.state === "allowed" ? "gold" : "default"}>
                  {b.name} · {t(STATE_LABEL[b.state])}
                </Chip>
              ))}
              {browsers.some((b) => b.state === "denied") && (
                <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => recorderClient.openSettings("automation")}>
                  {t("settings.allow_in", { app: platform.settingsApp })}
                </button>
              )}
            </div>
          )}
        </Row>
        <NotionRow settings={s} update={update} />
        <Row title={t("settings.speaker_names_title")} desc={t("settings.speaker_names_desc")}>
          <Switch aria-label={t("settings.speaker_names_aria")} checked={s.suggestSpeakerNames} onChange={(v) => update({ suggestSpeakerNames: v })} />
        </Row>
      </Group>
    </div>
  );
}

const NOTION_INTEGRATIONS = "https://www.notion.so/profile/integrations";

/** Notion: connect with an integration secret, then pick where meetings go. Sends only when you click Export. */
function NotionRow({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [places, setPlaces] = useState<NotionDestination[]>();

  useEffect(() => {
    if (!s.notion.connected) return;
    store.notionDestinations().then(setPlaces, (e) => setError(describeError(e)));
  }, [s.notion.connected, store]);

  const connect = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const workspace = await store.connectNotion(secret);
      setSecret("");
      toast(t("settings.notion_connected_toast", { workspace }));
    } catch (e) {
      setError(describeError(e));
    } finally {
      setBusy(false);
    }
  };

  if (!s.notion.connected) {
    return (
      <Row
        stack
        title={t("settings.notion_title")}
        desc={t("settings.notion_desc_off")}
      >
        <ol className="m-0 flex list-decimal flex-col gap-1 pl-5 text-xs leading-[17px] text-subtle">
          <li>
            {t("settings.notion_step1")}{" "}
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-ink underline" onClick={() => void openLink(NOTION_INTEGRATIONS)}>
              {t("settings.notion_step1_link")}
            </button>
          </li>
          <li>{t("settings.notion_step2")}</li>
          <li>{t("settings.notion_step3")}</li>
        </ol>
        <form
          className="flex flex-wrap items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void connect();
          }}
        >
          <Field label={t("settings.notion_secret")} type="password" placeholder="ntn_…" value={secret} onChange={(e) => setSecret(e.target.value)} wrapperClassName="w-72" />
          <Button type="submit" variant="primary" disabled={!secret.trim() || busy}>
            {busy ? t("settings.notion_checking") : t("settings.notion_connect")}
          </Button>
        </form>
        {error && <span className="text-xs text-warn-fg">{error}</span>}
      </Row>
    );
  }

  const current = s.notion.destination;
  return (
    <Row title={t("settings.notion_title")} desc={t("settings.notion_connected_desc", { workspace: s.notion.workspace ?? "Notion" })}>
      <div className="flex flex-wrap items-center gap-2">
        <Select
          label={t("settings.notion_destination")}
          wrapperClassName="w-56"
          value={current?.id ?? ""}
          onChange={(e) => update({ notion: { ...s.notion, destination: places?.find((p) => p.id === e.target.value) } })}
        >
          <option value="">{places === undefined ? t("settings.notion_loading") : places.length ? t("settings.notion_choose") : t("settings.notion_none")}</option>
          {places?.map((p) => (
            <option key={p.id} value={p.id}>
              {p.title} ({p.kind})
            </option>
          ))}
        </Select>
        <Button onClick={() => void store.disconnectNotion()}>{t("settings.notion_disconnect")}</Button>
      </div>
      {error && <span className="basis-full text-xs text-warn-fg">{error}</span>}
    </Row>
  );
}
