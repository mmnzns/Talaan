import { Button, Select, Switch, useToast } from "../../ui";
import { Group, Row } from "./parts";
import type { TabProps } from "./tabProps";

export function GeneralTab({ settings: s, update }: TabProps) {
  const toast = useToast();
  return (
    <Group>
      <Row title="Detect calls" desc="Notice when a call app starts using the mic. Set what happens per app in Call apps.">
        <Switch aria-label="Detect calls" checked={s.detectCalls} onChange={(v) => update({ detectCalls: v })} />
      </Row>
      <Row title="Open at login" desc="Keeps the menu bar icon ready.">
        <Switch aria-label="Open at login" checked={s.launchAtLogin} onChange={(v) => update({ launchAtLogin: v })} />
      </Row>
      <Row title="Recording notice" desc="Copied with one click when a recording starts." stack>
        <textarea
          aria-label="Recording notice"
          defaultValue={s.recordingNotice}
          onBlur={(e) => e.target.value !== s.recordingNotice && update({ recordingNotice: e.target.value })}
          rows={2}
          className="w-full resize-y rounded-control border border-input bg-paper p-2.5 font-[inherit] text-[13px] leading-[18px] text-ink outline-none focus:border-edge"
        />
      </Row>
      <Row title="Microphone" desc="Follows the system default. Switches automatically if AirPods connect mid-call.">
        <Select label="Microphone" wrapperClassName="w-52" value={s.microphone} onChange={(e) => update({ microphone: e.target.value })}>
          <option value="default">System default</option>
          <option value="MacBook Pro Microphone">MacBook Pro Microphone</option>
          <option value="AirPods Pro">AirPods Pro</option>
        </Select>
      </Row>
      <Row title="Warn when using speakers" desc="Recommends headphones so other voices aren't recorded twice.">
        <Switch aria-label="Warn when using speakers" checked={s.warnOnSpeakers} onChange={(v) => update({ warnOnSpeakers: v })} />
      </Row>
      <Row title="Appearance" desc="Auto matches your computer.">
        <Select label="Appearance" wrapperClassName="w-40" value={s.theme} onChange={(e) => update({ theme: e.target.value as TabProps["settings"]["theme"] })}>
          <option value="system">Auto</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </Select>
      </Row>
      <Row title="Notify when a summary is ready" desc="Click the notification to open the meeting.">
        <Switch aria-label="Notify when a summary is ready" checked={s.notifySummaryReady} onChange={(v) => update({ notifySummaryReady: v })} />
      </Row>
      <Row title="Updates" desc="Talaan 1.0.0-dev · up to date · checked today">
        <Button onClick={() => toast("Talaan is up to date")}>Check now</Button>
      </Row>
    </Group>
  );
}
