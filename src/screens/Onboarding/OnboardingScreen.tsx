import { useEffect, useState, type ReactNode } from "react";
import { Check } from "lucide-react";
import { Logo } from "../../brand/Logo";
import { useSettings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import type { Settings } from "../../data/settings";
import { Button, Chip, Switch, cn, useToast } from "../../ui";

const STEPS = ["Welcome", "Audio access", "Speech model", "Summaries", "Call apps", "Extras", "Done"] as const;

type Perm = "idle" | "allowed" | "denied";

export function OnboardingScreen() {
  const { settings, update } = useSettings();
  const [step, setStep] = useState(0);
  const [systemAudio, setSystemAudio] = useState<Perm>("idle");

  if (!settings) return null;
  const last = step === STEPS.length - 1;

  const finish = () => update({ onboardingDone: true });

  return (
    <div className="grid h-full place-items-center bg-paper p-6">
      <div className="flex h-[600px] max-h-full w-[780px] max-w-full overflow-hidden rounded-card border-[1.5px] border-edge bg-surface shadow-hard-lg">
        <aside className="flex w-[220px] flex-none flex-col gap-[18px] bg-aside px-[18px] py-[22px] text-aside-fg max-[640px]:hidden">
          <div className="flex items-center gap-2.5">
            <Logo size={28} />
            <span className="text-lg font-extrabold tracking-[-0.03em]">Talaan</span>
          </div>
          <ol className="m-0 flex list-none flex-col gap-1 p-0" aria-label="Setup steps">
            {STEPS.map((name, k) => (
              <li
                key={name}
                aria-current={k === step ? "step" : undefined}
                className={cn(
                  "flex items-center gap-2 rounded-control px-2 py-1.5 text-[13px]",
                  k === step ? "bg-lime font-bold text-on-lime" : k < step ? "text-aside-fg" : "text-aside-muted",
                )}
              >
                <span className="w-[18px] font-mono text-[11px]">{k < step ? "✓" : String(k + 1).padStart(2, "0")}</span>
                {name}
              </li>
            ))}
          </ol>
          <span className="mt-auto font-mono text-[11px] text-aside-muted">Everything stays on this Mac.</span>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col gap-4 overflow-auto px-8 pb-[22px] pt-[30px]">
          <div className="flex flex-1 flex-col gap-4">
            {step === 0 && <Welcome />}
            {step === 1 && <AudioAccess systemAudio={systemAudio} setSystemAudio={setSystemAudio} />}
            {step === 2 && <ModelStep settings={settings} update={update} />}
            {step === 3 && <SummaryStep settings={settings} update={update} />}
            {step === 4 && <AppsStep settings={settings} update={update} />}
            {step === 5 && <ExtrasStep settings={settings} update={update} />}
            {step === 6 && <Done systemAudio={systemAudio} />}
          </div>
          <div className="flex items-center gap-2">
            <span className="font-mono text-[11px] text-subtle">
              step {step + 1} of {STEPS.length}
            </span>
            <span className="flex-1" />
            {step > 0 && <Button variant="ghost" onClick={() => setStep(step - 1)}>Back</Button>}
            <Button variant="primary" raised onClick={() => (last ? finish() : setStep(step + 1))}>
              {step === 0 ? "Get started" : last ? "Open Talaan" : "Continue"}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

const h1 = "m-0 text-[26px] font-extrabold leading-[30px] tracking-[-0.035em]";
const desc = "m-0 text-[13px] leading-[18px] text-subtle";

function Option({ on, onClick, title, tag, children, role = "radio" }: { on: boolean; onClick?: () => void; title: string; tag?: string; children: ReactNode; role?: "radio" | "group" }) {
  return (
    <button
      type="button"
      role={role === "radio" ? "radio" : undefined}
      aria-checked={role === "radio" ? on : undefined}
      onClick={onClick}
      className={cn(
        "flex w-full cursor-pointer items-center gap-3 rounded-[10px] border-[1.5px] px-3 py-2.5 text-left font-[inherit] text-ink",
        on ? "border-edge bg-lime-soft" : "border-line bg-surface",
      )}
    >
      <span className="flex-1">
        <span className="flex items-center gap-2 font-bold">
          {title}
          {tag && <Chip tone="lime">{tag}</Chip>}
        </span>
        <span className="text-xs leading-[17px] text-subtle">{children}</span>
      </span>
    </button>
  );
}

function Welcome() {
  return (
    <>
      <span className="font-mono text-[11px] text-subtle">talaan · tagalog for “record”</span>
      <h1 className="m-0 text-[30px] font-extrabold leading-[34px] tracking-[-0.035em]">Meeting notes that stay on your Mac.</h1>
      <p className="m-0 text-[15px] leading-[22px] text-subtle">
        Talaan records both sides of any call, writes the transcript and a summary with decisions and action items, without a bot joining your meeting.
      </p>
      <div className="mt-1.5 grid grid-cols-3 gap-2.5 max-[860px]:grid-cols-1">
        {[
          ["No bot", "Works with any call app"],
          ["Offline", "Speech and AI on this Mac"],
          ["You decide", "Asks before it records"],
        ].map(([t, d]) => (
          <div key={t} className="flex flex-col gap-1 rounded-[10px] border-[1.5px] border-line bg-surface px-3 py-2.5">
            <span className="font-bold">{t}</span>
            <span className="text-xs text-subtle">{d}</span>
          </div>
        ))}
      </div>
    </>
  );
}

function PermRow({ title, desc: d, state, onAllow, onDeny }: { title: string; desc: string; state: Perm; onAllow?: () => void; onDeny?: () => void }) {
  return (
    <div className="flex items-center gap-3 rounded-[10px] border-[1.5px] border-line bg-surface px-3 py-2.5">
      <span className="flex-1">
        <span className="block font-bold">{title}</span>
        <span className="text-xs leading-[17px] text-subtle">{d}</span>
      </span>
      {state === "allowed" ? (
        <Chip tone="lime">
          <Check size={11} aria-hidden="true" /> Allowed
        </Chip>
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
  const toast = useToast();
  const [mic, setMic] = useState<Perm>("allowed");
  return (
    <>
      <h1 className={h1}>Allow two kinds of audio</h1>
      <p className={desc}>Talaan needs your microphone (your side) and system audio (everyone else). macOS will ask for each one next.</p>
      <PermRow title="Microphone" desc={`Records what you say. Labeled “Me”.`} state={mic} onAllow={() => setMic("allowed")} />
      <PermRow
        title="System audio"
        desc="Records the other people on the call. macOS calls this screen and system audio recording; Talaan only uses the audio."
        state={systemAudio}
        onAllow={() => setSystemAudio("allowed")}
        onDeny={() => setSystemAudio("denied")}
      />
      {systemAudio === "denied" ? (
        <div role="alert" className="flex flex-wrap items-center gap-3 rounded-control border-[1.5px] border-warn-fg bg-warn-bg px-3 py-2.5 text-[13px] text-warn-fg">
          <span className="flex-1 basis-56">
            <strong>System audio is off.</strong> Talaan will record your side only, so the transcript won’t include other people. Turn it on any time in System Settings › Privacy &amp; Security.
          </span>
          <Button onClick={() => toast("Opens System Settings")}>Open System Settings</Button>
        </div>
      ) : (
        <p className={desc}>If you say no, Talaan still works for your side only, and you can turn it on later in System Settings.</p>
      )}
    </>
  );
}

type StepProps = { settings: Settings; update: (p: Partial<Settings>) => Promise<Settings> };

function ModelStep({ settings: s, update }: StepProps) {
  const store = useSettingsStore();
  const options = s.speechModels.filter((m) => ["parakeet", "whisper-large", "nemotron"].includes(m.id));
  const info: Record<string, string> = {
    parakeet: "Fast and accurate, 25 European languages. Already on this Mac, no download.",
    "whisper-large": "Widest language support, including Filipino. Already on this Mac.",
    nemotron: "Most accurate on long English calls. 2.4 GB download.",
  };
  return (
    <>
      <h1 className={h1}>Choose a speech model</h1>
      <p className={desc}>Turns audio into text on this Mac. You can change it later.</p>
      <div role="radiogroup" aria-label="Speech model" className="flex flex-col gap-2">
        {options.map((m) => (
          <Option
            key={m.id}
            on={s.activeSpeechModel === m.id}
            title={m.name}
            tag={m.id === "parakeet" ? "recommended" : undefined}
            onClick={() => {
              update({ activeSpeechModel: m.id });
              if (m.state === "not_downloaded") store.downloadModel(m.id);
            }}
          >
            {m.state === "downloading" ? `${info[m.id]} Downloading ${Math.round((m.progress ?? 0) * 100)}%…` : info[m.id]}
          </Option>
        ))}
      </div>
    </>
  );
}

function SummaryStep({ settings: s, update }: StepProps) {
  const [pct, setPct] = useState(0);
  const local = s.summaryEngine === "local";

  useEffect(() => {
    if (!local) return;
    const timer = setInterval(() => setPct((p) => Math.min(100, p + 6)), 700);
    return () => clearInterval(timer);
  }, [local]);

  return (
    <>
      <h1 className={h1}>How summaries are written</h1>
      <p className={desc}>Every summary has the same format: recap, decisions, action items and open questions.</p>
      <div role="radiogroup" aria-label="Summary engine" className="flex flex-col gap-2">
        <Option on={local} title="Local AI" tag="default" onClick={() => update({ summaryEngine: "local" })}>
          Built in and offline. One-time 2.5 GB download.
        </Option>
        <Option on={s.summaryEngine === "byok"} title="Your own key" onClick={() => update({ summaryEngine: "byok" })}>
          Claude, OpenAI or Gemini. Sends transcript text only. Set up later in Settings.
        </Option>
      </div>
      {local && (
        <div className="flex items-center gap-2.5" role="status">
          <span aria-hidden="true" className="h-1.5 flex-1 overflow-hidden rounded-full bg-line">
            <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
          </span>
          <span className="font-mono text-[11px] text-subtle">
            {pct < 100 ? `downloading ${pct}% · keeps going in the background` : "Local AI is ready"}
          </span>
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
          <div key={id} className="flex items-center gap-3 rounded-[10px] border-[1.5px] border-line bg-surface px-3 py-2.5">
            <span className="flex-1 font-bold">{app.name}</span>
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
  const toast = useToast();
  const rows: [string, string, ReactNode][] = [
    ["Calendar", "Meeting titles and attendee names from your calendar.", <Switch key="c" aria-label="Calendar" checked={s.calendarConnected} onChange={(v) => update({ calendarConnected: v })} />],
    ["Browser call detection", "Recognize Google Meet, Zoom web and Teams web tabs.", <Switch key="b" aria-label="Browser call detection" checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />],
    [
      "Notion",
      "Send meetings to Notion when you choose to.",
      s.notion.connected ? (
        <Chip key="n" tone="lime">
          <Check size={11} aria-hidden="true" /> Connected
        </Chip>
      ) : (
        <Button
          key="n"
          onClick={() => {
            update({ notion: { connected: true, workspace: "Miguel", destination: "Meeting Notes (database)" } });
            toast("Connected to Notion");
          }}
        >
          Connect
        </Button>
      ),
    ],
  ];
  return (
    <>
      <h1 className={h1}>Optional extras</h1>
      <p className={desc}>All off by default. Turn on what helps, now or later in Settings.</p>
      {rows.map(([t, d, control]) => (
        <div key={t} className="flex items-center gap-3 rounded-[10px] border-[1.5px] border-line bg-surface px-3 py-2.5">
          <span className="flex-1">
            <span className="block font-bold">{t}</span>
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
      <h1 className="m-0 text-[30px] font-extrabold leading-[34px] tracking-[-0.035em]">You’re set.</h1>
      <p className="m-0 text-[15px] leading-[22px] text-subtle">
        Talaan lives in your menu bar. Join a call and it will ask to record, or click the menu bar icon to start.
      </p>
      <Option on={false} role="group" title="Tip: use headphones">
        So other people’s voices aren’t picked up twice by your mic.
      </Option>
      {systemAudio === "denied" && (
        <p className={desc}>Reminder: system audio is off, so only your side will be recorded until you allow it in System Settings.</p>
      )}
    </>
  );
}
