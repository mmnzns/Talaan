import { useEffect, useState } from "react";
import { recorderClient } from "../../data";
import type { BrowserState, CalendarAccess, NotionDestination } from "../../data/settings";
import { describeError, toStoreError } from "../../data/errors";
import { openLink } from "../../lib/windows";
import { useSettingsStore } from "../../data/StoreProvider";
import { Button, Chip, Field, Select, Switch, useToast } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";
import { onWindows, usePlatform } from "../../lib/platform";
import { tildePath } from "../../lib/format";
import { useT } from "../../lib/i18n/I18nProvider";
import type { MessageKey } from "../../lib/i18n";

const STATE_LABEL = {
  allowed: "settings.state_allowed",
  denied: "settings.state_denied",
  not_asked: "settings.state_not_asked",
  not_installed: "settings.state_not_installed",
} as const satisfies Record<BrowserState, MessageKey>;

// Windows calendar row: where each calendar shows its iCal address (brand names, not translated).
const ICS_HELP: [string, string][] = [
  ["Google Calendar", "https://support.google.com/calendar/answer/37648"],
  ["Outlook", "https://support.microsoft.com/office/share-your-calendar-in-outlook-on-the-web-7ecef8ae-139c-40d9-bae2-a23977ee58d5"],
  ["iCloud", "https://support.apple.com/guide/icloud/share-a-calendar-mm6b1a9479/icloud"],
];

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
        {onWindows ? (
          <CalendarAddressRow settings={s} update={update} />
        ) : (
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
        )}
        <Row title={t("settings.browser_title")} desc={onWindows ? t("settings.browser_desc_windows") : t("settings.browser_desc")}>
          <Switch aria-label={t("settings.browser_title")} checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />
          {s.tabDetection && (
            <div className="flex basis-full flex-wrap gap-2">
              {browsers.map((b) => (
                <Chip key={b.name} tone={b.state === "allowed" ? "gold" : "default"}>
                  {b.name} · {t(onWindows && b.state === "allowed" ? "settings.state_ready" : STATE_LABEL[b.state])}
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
        <AutoSaveRow settings={s} update={update} />
        <Row title={t("settings.speaker_names_title")} desc={t("settings.speaker_names_desc")}>
          <Switch aria-label={t("settings.speaker_names_aria")} checked={s.suggestSpeakerNames} onChange={(v) => update({ suggestSpeakerNames: v })} />
        </Row>
      </Group>
    </div>
  );
}

/** Windows: the calendar comes from its private iCal address. Connected = a tested address is saved. */
function CalendarAddressRow({ settings: s }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string }>();
  // "allowed" once an address is in secure key storage; the settings flag alone isn't enough.
  const [access, setAccess] = useState<CalendarAccess>();
  useEffect(() => {
    store.calendarAccess().then(setAccess, () => setAccess(undefined));
  }, [store, s.calendarConnected]);

  const connect = async () => {
    setBusy(true);
    setResult(undefined);
    try {
      const n = await store.connectCalendar(address);
      setAddress("");
      setResult({ ok: true, text: t("settings.ics_found", { count: n }) });
      toast(t("settings.ics_connected_toast"));
    } catch (e) {
      setResult({ ok: false, text: toStoreError(e).message });
    } finally {
      setBusy(false);
    }
  };

  if (s.calendarConnected && access !== "not_asked") {
    return (
      <Row title={t("settings.calendar_title")} desc={t("settings.ics_connected_desc")}>
        <Button
          onClick={() => {
            setResult(undefined);
            void store.disconnectCalendar();
          }}
        >
          {t("settings.ics_disconnect")}
        </Button>
        {result && <span className="basis-full text-xs text-subtle">{result.text}</span>}
      </Row>
    );
  }

  return (
    <Row stack title={t("settings.calendar_title")} desc={t("settings.ics_desc")}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-subtle">
        <span>{t("settings.ics_where")}</span>
        {ICS_HELP.map(([name, url]) => (
          <button key={name} type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-ink underline" onClick={() => void openLink(url)}>
            {name}
          </button>
        ))}
      </div>
      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void connect();
        }}
      >
        <Field
          label={t("settings.ics_field")}
          type="password"
          autoComplete="off"
          placeholder="https://…/basic.ics"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          wrapperClassName="w-80"
        />
        <Button type="submit" variant="primary" disabled={!address.trim() || busy}>
          {busy ? t("settings.ics_checking") : t("settings.ics_connect")}
        </Button>
      </form>
      {result && <span className={result.ok ? "text-xs text-subtle" : "text-xs text-warn-fg"}>{result.text}</span>}
    </Row>
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

/**
 * "Also save each meeting to a folder". It lives in Connections beside Notion because it's another place meetings
 * go, and it's where someone looks to send notes to Obsidian; Privacy lists it in "what leaves this computer".
 */
function AutoSaveRow({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const on = Boolean(s.autoSaveFolder);

  const choose = async () => {
    try {
      const folder = await store.pickFolder(t("settings.autosave_picker"));
      if (folder) await update({ autoSaveFolder: folder });
    } catch (e) {
      toast(describeError(e));
    }
  };

  return (
    <Row title={t("settings.autosave_title")} desc={t("settings.autosave_desc")}>
      <Switch aria-label={t("settings.autosave_title")} checked={on} onChange={(v) => void (v ? choose() : update({ autoSaveFolder: "" }))} />
      {on && (
        <div className="flex basis-full flex-col gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <span className="min-w-0 flex-1 basis-48 truncate font-mono text-xs text-subtle" title={s.autoSaveFolder}>
              {t("settings.autosave_saving_to", { path: tildePath(s.autoSaveFolder) })}
            </span>
            <Button onClick={() => void store.revealAutoSaveFolder().catch((e) => toast(describeError(e)))}>{t("settings.folder_open")}</Button>
            <Button onClick={() => void choose()}>{t("settings.autosave_change")}</Button>
          </div>
          <div className="flex items-center justify-between gap-5">
            <div>
              <div className="text-[13px] font-semibold">{t("settings.autosave_transcript")}</div>
              <div className="text-xs text-subtle">{t("settings.autosave_transcript_desc")}</div>
            </div>
            <Switch aria-label={t("settings.autosave_transcript")} checked={s.autoSaveTranscript} onChange={(v) => update({ autoSaveTranscript: v })} />
          </div>
        </div>
      )}
    </Row>
  );
}
