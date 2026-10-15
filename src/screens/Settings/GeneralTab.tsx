import { useState } from "react";
import { Logo } from "../../brand/Logo";
import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import { openLink } from "../../lib/windows";
import { Button, Label, Select, Switch } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

export function GeneralTab({ settings: s, update }: TabProps) {
  return (
    <div className="flex flex-col gap-6">
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
      <UpdatesRow settings={s} update={update} />
    </Group>
    <About />
    </div>
  );
}

type Check = { state: "idle" | "checking" | "current" | "installing" } | { state: "available"; version: string } | { state: "error"; message: string };

function UpdatesRow({ settings: s, update }: TabProps) {
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
      ? `Talaan ${check.version} is ready. Your meetings and settings stay.`
      : check.state === "current"
        ? `Talaan ${__APP_VERSION__} · up to date`
        : check.state === "error"
          ? check.message
          : check.state === "installing"
            ? "Downloading and installing. Talaan restarts by itself."
            : `Talaan ${__APP_VERSION__}`;
  return (
    <>
      <Row title="Updates" desc={desc}>
        {check.state === "available" ? (
          <Button variant="primary" onClick={() => void install()}>
            Update now
          </Button>
        ) : (
          <Button disabled={check.state === "checking" || check.state === "installing"} onClick={() => void run()}>
            {check.state === "checking" ? "Checking…" : "Check now"}
          </Button>
        )}
      </Row>
      <Row title="Check for updates automatically" desc="At launch, asks GitHub for the latest version number. Nothing about you or your meetings is sent.">
        <Switch aria-label="Check for updates automatically" checked={s.checkUpdates} onChange={(v) => update({ checkUpdates: v })} />
      </Row>
    </>
  );
}

const REPO = "https://github.com/mmnzns/Talaan";
const COFFEE = "https://buymeacoffee.com/mnmnzns";

function About() {
  return (
    <div className="flex flex-col gap-3">
      <SectionIntro title="About" />
      <div className="flex items-start gap-4 rounded-card border border-line bg-surface p-[22px] shadow-card">
        <Logo size={48} />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="font-display text-xl font-bold tracking-[-0.02em]">talaan</span>
          <Label>
            version {__APP_VERSION__} · mit license · tagalog for “record”
          </Label>
          <p className="m-0 mt-1 text-[13px] leading-[19px] text-subtle">
            A meeting recorder that stays on your computer. Free and open source, built in public by Miguel Monzones.
          </p>
          <div className="mt-2.5 flex flex-wrap gap-2">
            <Button variant="primary" onClick={() => void openLink(COFFEE)}>
              Buy me a coffee
            </Button>
            <Button onClick={() => void openLink(REPO)}>Source on GitHub</Button>
            <Button onClick={() => void openLink(`${REPO}/blob/main/THIRD_PARTY_NOTICES.md`)}>Credits</Button>
          </div>
        </div>
      </div>
    </div>
  );
}
