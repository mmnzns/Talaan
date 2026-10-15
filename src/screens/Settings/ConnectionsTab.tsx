import { useEffect, useState } from "react";
import { recorderClient } from "../../data";
import type { BrowserState, CalendarAccess, NotionDestination } from "../../data/settings";
import { describeError } from "../../data/errors";
import { openLink } from "../../lib/windows";
import { useSettingsStore } from "../../data/StoreProvider";
import { Button, Chip, Field, Select, Switch, useToast } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";
import { platform } from "../../lib/platform";

const STATE_LABEL = { allowed: "allowed", denied: "not allowed", not_asked: "asks at the first call", not_installed: "not installed" } as const;

export function ConnectionsTab({ settings: s, update }: TabProps) {
  const toast = useToast();
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
    else toast(`Calendar access is off for Talaan. Turn it on in ${platform.settingsApp}, then try again.`);
  };

  return (
    <div className="flex flex-col gap-3">
      <SectionIntro title="Connections" desc="All optional. Talaan works fully without them." />
      <Group>
        <Row title="Calendar" desc="Uses the event at recording time for the meeting title, attendees and speaker names. Reads your system calendar, on this computer only; never changes it.">
          <span className="flex items-center gap-3">
            {calendar === "denied" && (
              <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => recorderClient.openSettings("calendar")}>
                Allow in {platform.settingsApp}
              </button>
            )}
            <Switch aria-label="Calendar" checked={s.calendarConnected && calendar !== "denied"} onChange={toggleCalendar} />
          </span>
        </Row>
        <Row title="Browser call detection" desc="Recognizes Google Meet, Zoom web and Teams web tabs, so Browser calls can use Always. Your system asks once per browser.">
          <Switch aria-label="Browser call detection" checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />
          {s.tabDetection && (
            <div className="flex basis-full flex-wrap gap-2">
              {browsers.map((b) => (
                <Chip key={b.name} tone={b.state === "allowed" ? "gold" : "default"}>
                  {b.name} · {STATE_LABEL[b.state]}
                </Chip>
              ))}
              {browsers.some((b) => b.state === "denied") && (
                <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => recorderClient.openSettings("automation")}>
                  Allow in {platform.settingsApp}
                </button>
              )}
            </div>
          )}
        </Row>
        <NotionRow settings={s} update={update} />
        <Row title="Speaker names" desc="Suggest names for speakers from calendar attendees.">
          <Switch aria-label="Suggest speaker names" checked={s.suggestSpeakerNames} onChange={(v) => update({ suggestSpeakerNames: v })} />
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
      toast(`Connected to ${workspace}`);
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
        title="Notion"
        desc="Send a meeting to a Notion page or database when you click Export. Nothing is sent otherwise."
      >
        <ol className="m-0 flex list-decimal flex-col gap-1 pl-5 text-xs leading-[17px] text-subtle">
          <li>
            In Notion, make an internal integration named Talaan and copy its secret.{" "}
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-ink underline" onClick={() => void openLink(NOTION_INTEGRATIONS)}>
              Open Notion integrations
            </button>
          </li>
          <li>On each page or database Talaan may write to: ••• › Connections › add Talaan.</li>
          <li>Paste the secret here. It’s kept in your system’s secure key storage.</li>
        </ol>
        <form
          className="flex flex-wrap items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void connect();
          }}
        >
          <Field label="Notion integration secret" type="password" placeholder="ntn_…" value={secret} onChange={(e) => setSecret(e.target.value)} wrapperClassName="w-72" />
          <Button type="submit" variant="primary" disabled={!secret.trim() || busy}>
            {busy ? "Checking…" : "Connect"}
          </Button>
        </form>
        {error && <span className="text-xs text-warn-fg">{error}</span>}
      </Row>
    );
  }

  const current = s.notion.destination;
  return (
    <Row title="Notion" desc={`Connected to ${s.notion.workspace ?? "Notion"}. Sends only when you click Export.`}>
      <div className="flex flex-wrap items-center gap-2">
        <Select
          label="Default destination"
          wrapperClassName="w-56"
          value={current?.id ?? ""}
          onChange={(e) => update({ notion: { ...s.notion, destination: places?.find((p) => p.id === e.target.value) } })}
        >
          <option value="">{places === undefined ? "Loading…" : places.length ? "Choose where meetings go" : "Nothing shared with Talaan yet"}</option>
          {places?.map((p) => (
            <option key={p.id} value={p.id}>
              {p.title} ({p.kind})
            </option>
          ))}
        </Select>
        <Button onClick={() => void store.disconnectNotion()}>Disconnect</Button>
      </div>
      {error && <span className="basis-full text-xs text-warn-fg">{error}</span>}
    </Row>
  );
}
