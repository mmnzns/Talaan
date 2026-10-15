import { useEffect, useState } from "react";
import { recorderClient } from "../../data";
import type { BrowserState, CalendarAccess, NotionDestination } from "../../data/settings";
import { describeError, toStoreError } from "../../data/errors";
import { onWindows } from "../../lib/os";
import { openLink } from "../../lib/windows";
import { useSettingsStore } from "../../data/StoreProvider";
import { Button, Chip, Field, Select, Switch, useToast } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

const STATE_LABEL = { allowed: "allowed", denied: "not allowed", not_asked: "asks at the first call", not_installed: "not installed" } as const;

// Windows has no permission for this: Talaan reads the address bar of the tab in front of each browser window.
// Plain strings for now; they move to t() keys with i18n.
const TABS_DESC_WINDOWS = "Recognizes Google Meet, Zoom web and Teams web tabs, so Browser calls can use Always. Reads the address of the tab in front of each browser window, on this computer only.";
const STATE_LABEL_WINDOWS = { ...STATE_LABEL, allowed: "ready" } as const;
const stateLabel = onWindows ? STATE_LABEL_WINDOWS : STATE_LABEL;

// Windows calendar row (the calendar's iCal address). Plain strings for now; they move to t() keys with i18n.
const ICS_TEXT = {
  title: "Calendar",
  desc: "Uses the event at recording time for the meeting title, attendees and speaker names. Paste your calendar's private address in iCal format: Talaan downloads your calendar from it every 15 minutes, never changes it, and sends nothing.",
  connectedDesc: "Connected through your calendar's iCal address. Talaan downloads it every 15 minutes, never changes your calendar, and sends nothing.",
  where: "Where do I find this?",
  fieldLabel: "Calendar address in iCal format",
  placeholder: "https://…/basic.ics",
  connect: "Test and connect",
  checking: "Checking…",
  disconnect: "Disconnect",
  found: (n: number) => `Found ${n} ${n === 1 ? "event" : "events"} in the next 7 days.`,
  connectedToast: "Calendar connected",
} as const;

const ICS_HELP: [string, string][] = [
  ["Google Calendar", "https://support.google.com/calendar/answer/37648"],
  ["Outlook", "https://support.microsoft.com/office/share-your-calendar-in-outlook-on-the-web-7ecef8ae-139c-40d9-bae2-a23977ee58d5"],
  ["iCloud", "https://support.apple.com/guide/icloud/share-a-calendar-mm6b1a9479/icloud"],
];

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
    else toast("Calendar access is off for Talaan. Turn it on in your system settings, then try again.");
  };

  return (
    <div className="flex flex-col gap-3">
      <SectionIntro title="Connections" desc="All optional. Talaan works fully without them." />
      <Group>
        {onWindows ? (
          <CalendarAddressRow settings={s} update={update} />
        ) : (
          <Row title="Calendar" desc="Uses the event at recording time for the meeting title, attendees and speaker names. Reads your system calendar, on this computer only; never changes it.">
            <span className="flex items-center gap-3">
              {calendar === "denied" && (
                <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => recorderClient.openSettings("calendar")}>
                  Allow in system settings
                </button>
              )}
              <Switch aria-label="Calendar" checked={s.calendarConnected && calendar !== "denied"} onChange={toggleCalendar} />
            </span>
          </Row>
        )}
        <Row
          title="Browser call detection"
          desc={onWindows ? TABS_DESC_WINDOWS : "Recognizes Google Meet, Zoom web and Teams web tabs, so Browser calls can use Always. Your system asks once per browser."}
        >
          <Switch aria-label="Browser call detection" checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />
          {s.tabDetection && (
            <div className="flex basis-full flex-wrap gap-2">
              {browsers.map((b) => (
                <Chip key={b.name} tone={b.state === "allowed" ? "gold" : "default"}>
                  {b.name} · {stateLabel[b.state]}
                </Chip>
              ))}
              {browsers.some((b) => b.state === "denied") && (
                <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => recorderClient.openSettings("automation")}>
                  Allow in system settings
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

/** Windows: the calendar comes from its private iCal address. Connected = a tested address is saved. */
function CalendarAddressRow({ settings: s }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
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
      setResult({ ok: true, text: ICS_TEXT.found(n) });
      toast(ICS_TEXT.connectedToast);
    } catch (e) {
      setResult({ ok: false, text: toStoreError(e).message });
    } finally {
      setBusy(false);
    }
  };

  if (s.calendarConnected && access !== "not_asked") {
    return (
      <Row title={ICS_TEXT.title} desc={ICS_TEXT.connectedDesc}>
        <Button
          onClick={() => {
            setResult(undefined);
            void store.disconnectCalendar();
          }}
        >
          {ICS_TEXT.disconnect}
        </Button>
        {result && <span className="basis-full text-xs text-subtle">{result.text}</span>}
      </Row>
    );
  }

  return (
    <Row stack title={ICS_TEXT.title} desc={ICS_TEXT.desc}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-subtle">
        <span>{ICS_TEXT.where}</span>
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
          label={ICS_TEXT.fieldLabel}
          type="password"
          autoComplete="off"
          placeholder={ICS_TEXT.placeholder}
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          wrapperClassName="w-80"
        />
        <Button type="submit" variant="primary" disabled={!address.trim() || busy}>
          {busy ? ICS_TEXT.checking : ICS_TEXT.connect}
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
