import { useEffect, useState, type ReactNode } from "react";
import { Check, Headphones } from "lucide-react";
import { Logo } from "../../brand/Logo";
import { recorderClient } from "../../data";
import { useSettings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import type { Settings } from "../../data/settings";
import { Button, Chip, Label, Radio, Switch, cn, useToast } from "../../ui";
import { SunCorner } from "../../brand/SunCorner";
import { platform } from "../../lib/platform";

const STEPS = ["Welcome", "Audio access", "Speech model", "Summaries", "Call apps", "Extras", "Done"] as const;

/**
 * "asked": the system prompt was shown; its answer can't be read, only seen in the first recording.
 * "not_needed": the system has no permission for it (system audio on Windows).
 */
type Perm = "idle" | "allowed" | "asked" | "denied" | "not_needed";

export function OnboardingScreen() {
  const { settings, update } = useSettings();
  const [step, setStep] = useState(0);
  const [systemAudio, setSystemAudio] = useState<Perm>(platform.systemAudioNeedsPermission ? "idle" : "not_needed");

  if (!settings) return null;
  const last = step === STEPS.length - 1;

  const finish = () => update({ onboardingDone: true });

  const next = () => (last ? finish() : setStep(step + 1));

  return (
    <div className="grid h-full place-items-center bg-paper p-6">
      <div className="relative flex h-[620px] max-h-full w-[800px] max-w-full flex-col overflow-hidden rounded-[18px] border border-line bg-surface shadow-[0_16px_40px_rgba(17,19,24,0.12)]">
        <SunCorner size={step === 0 ? 220 : 170} className="absolute right-0 top-0" />
        {step === 0 ? (
          <>
            <div className="relative flex flex-1 flex-col items-center gap-5 px-16 pt-14 text-center">
              <Welcome />
            </div>
            <div className="relative flex justify-center pb-[34px]">
              <Button variant="primary" className="h-[42px] px-6 text-sm" onClick={next}>
                Get started
              </Button>
            </div>
          </>
        ) : (
          <>
            <div className="relative flex items-center gap-2.5 px-7 py-[22px]">
              <Logo size={28} />
              <span className="font-display text-[17px] font-bold tracking-[-0.02em]">talaan</span>
              <span className="flex-1" />
              <ol className="m-0 mr-24 flex list-none gap-1.5 p-0" aria-label="Setup steps">
                {STEPS.map((name, k) => (
                  <li
                    key={name}
                    aria-label={name}
                    aria-current={k === step ? "step" : undefined}
                    className={cn("h-1.5 rounded-full transition-[width]", k === step ? "w-[22px]" : "w-1.5", k <= step ? "bg-gold" : "bg-edge")}
                  />
                ))}
              </ol>
            </div>
            <div className="relative flex min-h-0 flex-1 flex-col gap-[26px] overflow-auto px-16 pt-[18px]">
              {step === 1 && <AudioAccess systemAudio={systemAudio} setSystemAudio={setSystemAudio} />}
              {step === 2 && <ModelStep settings={settings} update={update} />}
              {step === 3 && <SummaryStep settings={settings} update={update} />}
              {step === 4 && <AppsStep settings={settings} update={update} />}
              {step === 5 && <ExtrasStep settings={settings} update={update} />}
              {step === 6 && <Done systemAudio={systemAudio} />}
            </div>
            <div className="relative flex items-center gap-2 border-t border-line px-7 py-[18px]">
              <Label>
                step {step + 1} of {STEPS.length} · {STEPS[step]}
              </Label>
              <span className="flex-1" />
              <Button variant="ghost" onClick={() => setStep(step - 1)}>
                Back
              </Button>
              <Button variant="primary" onClick={next}>
                {last ? "Open Talaan" : "Continue"}
              </Button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

const h1 = "m-0 font-display text-[30px] font-bold leading-[34px] tracking-[-0.02em]";
const desc = "m-0 max-w-[520px] text-sm leading-[21px] text-subtle";

function Option({ on, onClick, title, tag, children, role = "radio" }: { on: boolean; onClick?: () => void; title: string; tag?: string; children: ReactNode; role?: "radio" | "group" }) {
  return (
    <button
      type="button"
      role={role === "radio" ? "radio" : undefined}
      aria-checked={role === "radio" ? on : undefined}
      onClick={onClick}
      className={cn(
        "flex w-full cursor-pointer items-center gap-3.5 rounded-card border px-4 py-3.5 text-left font-[inherit] text-ink",
        on ? "border-gold bg-surface shadow-[0_0_0_3px_var(--t-gold-soft)]" : "border-line bg-surface hover:border-edge",
      )}
    >
      {role === "radio" && <Radio checked={on} />}
      <span className="flex-1">
        <span className="flex items-center gap-2 font-bold">
          {title}
          {tag && <Chip tone="gold">{tag}</Chip>}
        </span>
        <span className="text-xs leading-[17px] text-subtle">{children}</span>
      </span>
    </button>
  );
}

function Welcome() {
  return (
    <>
      <div className="flex items-center gap-3.5">
        <Logo size={56} />
        <span className="font-display text-[44px] font-bold leading-none tracking-[-0.02em]">talaan</span>
      </div>
      <Label className="text-balance">tagalog for “record” · the logo is “ta” in baybayin, the native filipino script</Label>
      <h1 className="m-0 max-w-[540px] font-display text-[40px] font-bold leading-[44px] tracking-[-0.02em]">Meeting notes that stay on your computer.</h1>
      <p className="m-0 max-w-[470px] text-[15px] leading-[23px] text-subtle">
        Records both sides of any call, then writes the transcript and a summary with decisions and action items. No bot joins your meeting.
      </p>
      <div className="flex flex-wrap justify-center gap-2">
        <Chip>no bot</Chip>
        <Chip>fully offline</Chip>
        <Chip>asks before it records</Chip>
      </div>
    </>
  );
}

function PermRow({ title, desc: d, state, onAllow, onDeny, onOpen }: { title: string; desc: string; state: Perm; onAllow?: () => void; onDeny?: () => void; onOpen?: () => void }) {
  return (
    <div className="flex items-center gap-3.5 rounded-card border border-line bg-surface px-[18px] py-4 shadow-card">
      <span className="flex-1">
        <span className="block font-semibold">{title}</span>
        <span className="text-xs leading-[17px] text-subtle">{d}</span>
      </span>
      {state === "allowed" || state === "asked" || state === "not_needed" ? (
        <Chip tone="gold">
          <Check size={11} aria-hidden="true" /> {state === "asked" ? "asked" : "allowed"}
        </Chip>
      ) : state === "denied" && onOpen ? (
        <Button onClick={onOpen}>{platform.openSettings}</Button>
      ) : (
        <span className="flex gap-2">
          {onDeny && <Button variant="ghost" onClick={onDeny}>Not now</Button>}
          <Button variant="primary" onClick={onAllow}>Allow</Button>
        </span>
      )}
    </div>
  );
}

function AudioAccess({ systemAudio, setSystemAudio }: { systemAudio: Perm; setSystemAudio: (p: Perm) => void }) {
  const [mic, setMic] = useState<Perm>("idle");
  useEffect(() => {
    recorderClient.microphonePermission().then((p) => setMic(p === "granted" ? "allowed" : p === "denied" || p === "restricted" ? "denied" : "idle"));
  }, []);
  const askMic = async () => setMic((await recorderClient.requestMicrophone()) ? "allowed" : "denied");
  const askSystem = async () => setSystemAudio((await recorderClient.requestSystemAudio()) ? "asked" : "denied");
  return (
    <>
      <h1 className={h1}>Allow two kinds of audio</h1>
      <p className={desc}>{platform.audioIntro}</p>
      <PermRow title="Microphone" desc={`Records what you say. Labeled “Me”.`} state={mic} onAllow={() => void askMic()} onOpen={() => void recorderClient.openSettings("microphone")} />
      <PermRow
        title="System audio"
        desc={platform.systemAudioDesc}
        state={systemAudio}
        onAllow={() => void askSystem()}
        onDeny={() => setSystemAudio("denied")}
        onOpen={() => void recorderClient.openSettings("system-audio")}
      />
      {systemAudio === "denied" ? (
        <div role="alert" className="flex flex-wrap items-center gap-3 rounded-card bg-warn-bg px-3 py-2.5 text-[13px] text-warn-fg">
          <span className="flex-1 basis-56">
            <strong>System audio is off.</strong> Talaan will record your side only, so the transcript won’t include other people. Turn it on any time in {platform.settingsApp} › Privacy &amp; Security.
          </span>
          <Button onClick={() => void recorderClient.openSettings("system-audio")}>{platform.openSettings}</Button>
        </div>
      ) : systemAudio === "asked" ? (
        <p className={desc}>If you chose Don’t Allow, Talaan records your side only until you turn it on in {platform.settingsApp}. The first recording tells you if the other side can’t be heard.</p>
      ) : systemAudio === "idle" ? (
        <p className={desc}>If you say no, Talaan still works for your side only, and you can turn it on later in {platform.settingsApp}.</p>
      ) : null}
    </>
  );
}

type StepProps = { settings: Settings; update: (p: Partial<Settings>) => Promise<Settings> };

function ModelStep({ settings: s, update }: StepProps) {
  const store = useSettingsStore();
  const status = (m: Settings["speechModels"][number]) => {
    if (m.state === "on_this_mac") return "Already on this computer, no download.";
    if (m.state === "downloaded") return "Downloaded.";
    if (m.state === "downloading") return `Downloading ${Math.round((m.progress ?? 0) * 100)}%…`;
    return `${(m.sizeMb / 1000).toFixed(1)} GB download.`;
  };
  return (
    <>
      <h1 className={h1}>Choose a speech model</h1>
      <p className={desc}>Turns audio into text on this computer. You can change it later.</p>
      <div role="radiogroup" aria-label="Speech model" className="flex flex-col gap-2">
        {s.speechModels.map((m) => (
          <Option
            key={m.id}
            on={s.activeSpeechModel === m.id}
            title={m.name}
            tag={m.id === "whisper-turbo" ? "recommended" : undefined}
            onClick={() => {
              update({ activeSpeechModel: m.id });
              if (m.state === "not_downloaded") store.downloadModel(m.id);
            }}
          >
            {m.note} {status(m)}
          </Option>
        ))}
      </div>
    </>
  );
}

function SummaryStep({ settings: s, update }: StepProps) {
  const store = useSettingsStore();
  const local = s.summaryEngine === "local";
  const model = s.localModels.find((m) => m.choice === (s.localModel ?? "recommended")) ?? s.localModels[0];

  // Choosing Local AI starts its one-time download; it keeps going in the background.
  useEffect(() => {
    if (local && model?.state === "not_downloaded") void store.downloadModel(model.id);
  }, [local, model?.id, model?.state, store]);

  const pct = Math.round((model?.progress ?? 0) * 100);
  return (
    <>
      <h1 className={h1}>How summaries are written</h1>
      <p className={desc}>Every summary has the same format: recap, decisions, action items and open questions.</p>
      <div role="radiogroup" aria-label="Summary engine" className="flex flex-col gap-2">
        <Option on={local} title="On this computer" tag="default" onClick={() => update({ summaryEngine: "local" })}>
          Built in and offline. One-time {model ? `${(model.sizeMb / 1000).toFixed(1)} GB` : ""} download.
        </Option>
        <Option on={s.summaryEngine === "byok"} title="Your own key" onClick={() => update({ summaryEngine: "byok" })}>
          Claude, OpenAI, Gemini or any OpenAI-compatible service. Sends transcript text only. Set up later in Settings.
        </Option>
      </div>
      {local && model && (
        <div className="flex items-center gap-2.5" role="status">
          <span aria-hidden="true" className="h-1.5 flex-1 overflow-hidden rounded-full bg-line">
            <span className="block h-full bg-gold" style={{ width: `${model.state === "downloaded" ? 100 : pct}%` }} />
          </span>
          <Label>{model.state === "downloaded" ? "local ai is ready" : `downloading ${pct}% · keeps going in the background`}</Label>
        </div>
      )}
    </>
  );
}

function AppsStep({ settings: s, update }: StepProps) {
  const setAlways = (id: string, always: boolean) =>
    update({ callApps: s.callApps.map((a) => (a.id === id ? { ...a, rule: always ? "always" : "ask" } : a)) });
  return (
    <>
      <h1 className={h1}>When should Talaan record?</h1>
      <p className={desc}>By default it asks every time a call app uses your mic. Pick apps you always want recorded.</p>
      {["zoom", "slack", "teams"].map((id) => {
        const app = s.callApps.find((a) => a.id === id);
        if (!app) return null;
        return (
          <div key={id} className="flex items-center gap-3 rounded-card border border-line bg-surface px-[18px] py-4 shadow-card">
            <span className="flex-1 font-semibold">{app.name}</span>
            <span className="text-[13px]">Always record</span>
            <Switch aria-label={`Always record ${app.name}`} checked={app.rule === "always"} onChange={(v) => setAlways(id, v)} />
          </div>
        );
      })}
      <p className={desc}>Dictation apps (Parlato, VoiceInk, Superwhisper and others) are ignored automatically.</p>
    </>
  );
}

function ExtrasStep({ settings: s, update }: StepProps) {
  const rows: [string, string, ReactNode][] = [
    ["Calendar", "Meeting titles and attendee names from your calendar.", <CalendarSwitch key="c" settings={s} update={update} />],
    ["Browser call detection", "Recognize Google Meet, Zoom web and Teams web tabs.", <Switch key="b" aria-label="Browser call detection" checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />],
    [
      "Notion",
      "Send meetings to Notion when you choose to.",
      s.notion.connected ? (
        <Chip key="n" tone="gold">
          <Check size={11} aria-hidden="true" /> Connected
        </Chip>
      ) : (
        <Label key="n">later in settings › connections</Label>
      ),
    ],
  ];
  return (
    <>
      <h1 className={h1}>Optional extras</h1>
      <p className={desc}>All off by default. Turn on what helps, now or later in Settings.</p>
      {rows.map(([t, d, control]) => (
        <div key={t} className="flex items-center gap-3 rounded-card border border-line bg-surface px-[18px] py-4 shadow-card">
          <span className="flex-1">
            <span className="block font-semibold">{t}</span>
            <span className="text-xs text-subtle">{d}</span>
          </span>
          {control}
        </div>
      ))}
    </>
  );
}

function Done({ systemAudio }: { systemAudio: Perm }) {
  return (
    <>
      <h1 className={h1}>You’re set.</h1>
      <p className={desc}>Talaan lives in your {platform.trayName}. Join a call and it will ask to record, or click the Talaan icon to start.</p>
      <div className="flex items-center gap-3.5 rounded-card bg-sp1 px-[18px] py-4">
        <Headphones size={22} aria-hidden="true" className="flex-none text-sp1-fg" />
        <div>
          <div className="font-semibold text-sp1-fg">Tip: use headphones</div>
          <div className="mt-0.5 text-xs leading-[17px] text-subtle">So other people’s voices aren’t picked up twice by your mic.</div>
        </div>
      </div>
      {systemAudio === "denied" && (
        <p className={desc}>Reminder: system audio is off, so only your side will be recorded until you allow it in {platform.settingsApp}.</p>
      )}
    </>
  );
}

function CalendarSwitch({ settings: s, update }: StepProps) {
  const store = useSettingsStore();
  const toast = useToast();
  return (
    <Switch
      aria-label="Calendar"
      checked={s.calendarConnected}
      onChange={async (on) => {
        if (!on) return void update({ calendarConnected: false });
        if ((await store.requestCalendar()) === "allowed") void update({ calendarConnected: true });
        else toast(`Calendar access is off for Talaan. You can turn it on later in ${platform.settingsApp}.`);
      }}
    />
  );
}
