import { useEffect, useState } from "react";
import { recorderClient } from "../../data";
import type { CalendarAccess } from "../../data/settings";
import { useSettingsStore } from "../../data/StoreProvider";
import { Button, Chip, Select, Switch, useToast } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

const DESTINATIONS = ["Meeting Notes (database)", "Talaan Journey (page)"];
const STATE_LABEL = { allowed: "allowed", not_asked: "not asked yet", not_installed: "not installed" } as const;

export function ConnectionsTab({ settings: s, update }: TabProps) {
  const toast = useToast();
  const store = useSettingsStore();
  const [calendar, setCalendar] = useState<CalendarAccess>();
  useEffect(() => {
    store.calendarAccess().then(setCalendar, () => setCalendar(undefined));
  }, [store]);

  const toggleCalendar = async (on: boolean) => {
    if (!on) return update({ calendarConnected: false });
    const access = await store.requestCalendar();
    setCalendar(access);
    if (access === "allowed") update({ calendarConnected: true });
    else toast("Calendar access is off for Talaan. Turn it on in your system settings, then try again.");
  };

  const connect = () => {
    update({ notion: { connected: true, workspace: "Miguel", destination: DESTINATIONS[0] } });
    toast("Connected to Notion");
  };

  return (
    <div className="flex flex-col gap-3">
      <SectionIntro title="Connections" desc="All optional. Talaan works fully without them." />
      <Group>
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
        <Row title="Browser call detection" desc="Recognizes Google Meet, Zoom web and Teams web tabs, so Browser calls can use Always. Your system asks once per browser.">
          <Switch aria-label="Browser call detection" checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />
          {s.tabDetection && (
            <div className="flex basis-full flex-wrap gap-2">
              {s.browsers.map((b) => (
                <Chip key={b.name} tone={b.state === "allowed" ? "lime" : "default"}>
                  {b.name} · {STATE_LABEL[b.state]}
                </Chip>
              ))}
            </div>
          )}
        </Row>
        <Row
          title="Notion"
          desc={
            s.notion.connected
              ? `Connected as ${s.notion.workspace ?? "you"}. Sends only when you click Export.`
              : "Not connected. Send a meeting to a Notion page or database when you click Export."
          }
        >
          {s.notion.connected ? (
            <div className="flex items-center gap-2">
              <Select
                label="Default destination"
                wrapperClassName="w-52"
                value={s.notion.destination ?? DESTINATIONS[0]}
                onChange={(e) => update({ notion: { ...s.notion, destination: e.target.value } })}
              >
                {DESTINATIONS.map((d) => (
                  <option key={d}>{d}</option>
                ))}
              </Select>
              <Button onClick={() => update({ notion: { connected: false } })}>Disconnect</Button>
            </div>
          ) : (
            <Button variant="primary" onClick={connect}>
              Connect Notion
            </Button>
          )}
        </Row>
        <Row title="Speaker names" desc="Suggest names for speakers from calendar attendees.">
          <Switch aria-label="Suggest speaker names" checked={s.suggestSpeakerNames} onChange={(v) => update({ suggestSpeakerNames: v })} />
        </Row>
      </Group>
    </div>
  );
}
