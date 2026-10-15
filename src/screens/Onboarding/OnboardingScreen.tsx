import { useEffect, useState, type ReactNode } from "react";
import { Check, Headphones } from "lucide-react";
import { Logo } from "../../brand/Logo";
import { recorderClient } from "../../data";
import { useSettings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import type { Settings } from "../../data/settings";
import { Button, Chip, Label, Radio, Switch, cn, useToast } from "../../ui";
import { SunCorner } from "../../brand/SunCorner";
import { onWindows, systemAudioNeedsPermission, usePlatform } from "../../lib/platform";
import { useT } from "../../lib/i18n/I18nProvider";

const STEPS = ["step0", "step1", "step2", "step3", "step4", "step5", "step6"] as const;

/**
 * "asked": the system prompt was shown; its answer can't be read, only seen in the first recording.
 * "not_needed": the system has no permission for it (system audio on Windows).
 */
type Perm = "idle" | "allowed" | "asked" | "denied" | "not_needed";

export function OnboardingScreen() {
  const t = useT();
  const { settings, update } = useSettings();
  const [step, setStep] = useState(0);
  const [systemAudio, setSystemAudio] = useState<Perm>(systemAudioNeedsPermission ? "idle" : "not_needed");

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
                {t("onboarding.getStarted")}
              </Button>
            </div>
          </>
        ) : (
          <>
            <div className="relative flex items-center gap-2.5 px-7 py-[22px]">
              <Logo size={28} />
              <span className="font-display text-[17px] font-bold tracking-[-0.02em]">talaan</span>
              <span className="flex-1" />
              <ol className="m-0 mr-24 flex list-none gap-1.5 p-0" aria-label={t("onboarding.setupSteps")}>
                {STEPS.map((name, k) => (
                  <li
                    key={name}
                    aria-label={t(`onboarding.${name}`)}
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
                {t("onboarding.stepOf", { n: step + 1, total: STEPS.length, name: t(`onboarding.${STEPS[step]}`) })}
              </Label>
              <span className="flex-1" />
              <Button variant="ghost" onClick={() => setStep(step - 1)}>
                {t("onboarding.back")}
              </Button>
              <Button variant="primary" onClick={next}>
                {last ? t("onboarding.openTalaan") : t("onboarding.continue")}
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
  const t = useT();
  return (
    <>
      <div className="flex items-center gap-3.5">
        <Logo size={56} />
        <span className="font-display text-[44px] font-bold leading-none tracking-[-0.02em]">talaan</span>
      </div>
      <Label className="text-balance">{t("onboarding.welcomeOrigin")}</Label>
      <h1 className="m-0 max-w-[540px] font-display text-[40px] font-bold leading-[44px] tracking-[-0.02em]">{t("onboarding.welcomeTitle")}</h1>
      <p className="m-0 max-w-[470px] text-[15px] leading-[23px] text-subtle">
        {t("onboarding.welcomeDesc")}
      </p>
      <div className="flex flex-wrap justify-center gap-2">
        <Chip>{t("onboarding.chipNoBot")}</Chip>
        <Chip>{t("onboarding.chipOffline")}</Chip>
        <Chip>{t("onboarding.chipAsks")}</Chip>
      </div>
    </>
  );
}

function PermRow({ title, desc: d, state, onAllow, onDeny, onOpen }: { title: string; desc: string; state: Perm; onAllow?: () => void; onDeny?: () => void; onOpen?: () => void }) {
  const t = useT();
  const platform = usePlatform();
  return (
    <div className="flex items-center gap-3.5 rounded-card border border-line bg-surface px-[18px] py-4 shadow-card">
      <span className="flex-1">
        <span className="block font-semibold">{title}</span>
        <span className="text-xs leading-[17px] text-subtle">{d}</span>
      </span>
      {state === "allowed" || state === "asked" || state === "not_needed" ? (
        <Chip tone="gold">
          <Check size={11} aria-hidden="true" /> {state === "asked" ? t("onboarding.asked") : t("onboarding.allowed")}
        </Chip>
      ) : state === "denied" && onOpen ? (
        <Button onClick={onOpen}>{platform.openSettings}</Button>
      ) : (
        <span className="flex gap-2">
          {onDeny && <Button variant="ghost" onClick={onDeny}>{t("onboarding.notNow")}</Button>}
          <Button variant="primary" onClick={onAllow}>{t("onboarding.allow")}</Button>
        </span>
      )}
    </div>
  );
}

function AudioAccess({ systemAudio, setSystemAudio }: { systemAudio: Perm; setSystemAudio: (p: Perm) => void }) {
  const t = useT();
  const platform = usePlatform();
  const [mic, setMic] = useState<Perm>("idle");
  useEffect(() => {
    recorderClient.microphonePermission().then((p) => setMic(p === "granted" ? "allowed" : p === "denied" || p === "restricted" ? "denied" : "idle"));
  }, []);
  const askMic = async () => setMic((await recorderClient.requestMicrophone()) ? "allowed" : "denied");
  const askSystem = async () => setSystemAudio((await recorderClient.requestSystemAudio()) ? "asked" : "denied");
  return (
    <>
      <h1 className={h1}>{t("onboarding.audioTitle")}</h1>
      <p className={desc}>{platform.audioIntro}</p>
      <PermRow title={t("onboarding.micTitle")} desc={t("onboarding.micDesc")} state={mic} onAllow={() => void askMic()} onOpen={() => void recorderClient.openSettings("microphone")} />
      <PermRow
        title={t("onboarding.systemTitle")}
        desc={platform.systemAudioDesc}
        state={systemAudio}
        onAllow={() => void askSystem()}
        onDeny={() => setSystemAudio("denied")}
        onOpen={() => void recorderClient.openSettings("system-audio")}
      />
      {systemAudio === "denied" ? (
        <div role="alert" className="flex flex-wrap items-center gap-3 rounded-card bg-warn-bg px-3 py-2.5 text-[13px] text-warn-fg">
          <span className="flex-1 basis-56">
            <strong>{t("onboarding.systemOffLead")}</strong> {t("onboarding.systemOffRest", { app: platform.settingsApp })}
          </span>
          <Button onClick={() => void recorderClient.openSettings("system-audio")}>{platform.openSettings}</Button>
        </div>
      ) : systemAudio === "asked" ? (
        <p className={desc}>{t("onboarding.systemAsked", { app: platform.settingsApp })}</p>
      ) : systemAudio === "idle" ? (
        <p className={desc}>{t("onboarding.systemIdle", { app: platform.settingsApp })}</p>
      ) : null}
    </>
  );
}

type StepProps = { settings: Settings; update: (p: Partial<Settings>) => Promise<Settings> };

function ModelStep({ settings: s, update }: StepProps) {
  const t = useT();
  const store = useSettingsStore();
  const status = (m: Settings["speechModels"][number]) => {
    if (m.state === "on_this_mac") return t("onboarding.modelOnThisComputer");
    if (m.state === "downloaded") return t("onboarding.modelDownloaded");
    if (m.state === "downloading") return t("onboarding.modelDownloading", { pct: Math.round((m.progress ?? 0) * 100) });
    return t("onboarding.modelSize", { size: (m.sizeMb / 1000).toFixed(1) });
  };
  return (
    <>
      <h1 className={h1}>{t("onboarding.modelTitle")}</h1>
      <p className={desc}>{t("onboarding.modelDesc")}</p>
      <div role="radiogroup" aria-label={t("onboarding.modelGroup")} className="flex flex-col gap-2">
        {s.speechModels.map((m) => (
          <Option
            key={m.id}
            on={s.activeSpeechModel === m.id}
            title={m.name}
            tag={m.id === "whisper-turbo" ? t("onboarding.recommended") : undefined}
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
  const t = useT();
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
      <h1 className={h1}>{t("onboarding.summaryTitle")}</h1>
      <p className={desc}>{t("onboarding.summaryDesc")}</p>
      <div role="radiogroup" aria-label={t("onboarding.summaryGroup")} className="flex flex-col gap-2">
        <Option on={local} title={t("onboarding.localTitle")} tag={t("onboarding.defaultTag")} onClick={() => update({ summaryEngine: "local" })}>
          {model ? t("onboarding.localDesc", { size: (model.sizeMb / 1000).toFixed(1) }) : t("onboarding.localDescNoSize")}
        </Option>
        <Option on={s.summaryEngine === "byok"} title={t("onboarding.byokTitle")} onClick={() => update({ summaryEngine: "byok" })}>
          {t("onboarding.byokDesc")}
        </Option>
      </div>
      {local && model && (
        <div className="flex items-center gap-2.5" role="status">
          <span aria-hidden="true" className="h-1.5 flex-1 overflow-hidden rounded-full bg-line">
            <span className="block h-full bg-gold" style={{ width: `${model.state === "downloaded" ? 100 : pct}%` }} />
          </span>
          <Label>{model.state === "downloaded" ? t("onboarding.localReady") : t("onboarding.localDownloading", { pct })}</Label>
        </div>
      )}
    </>
  );
}

function AppsStep({ settings: s, update }: StepProps) {
  const t = useT();
  const setAlways = (id: string, always: boolean) =>
    update({ callApps: s.callApps.map((a) => (a.id === id ? { ...a, rule: always ? "always" : "ask" } : a)) });
  return (
    <>
      <h1 className={h1}>{t("onboarding.appsTitle")}</h1>
      <p className={desc}>{t("onboarding.appsDesc")}</p>
      {["zoom", "slack", "teams"].map((id) => {
        const app = s.callApps.find((a) => a.id === id);
        if (!app) return null;
        return (
          <div key={id} className="flex items-center gap-3 rounded-card border border-line bg-surface px-[18px] py-4 shadow-card">
            <span className="flex-1 font-semibold">{app.name}</span>
            <span className="text-[13px]">{t("onboarding.alwaysRecord")}</span>
            <Switch aria-label={t("onboarding.alwaysRecordApp", { app: app.name })} checked={app.rule === "always"} onChange={(v) => setAlways(id, v)} />
          </div>
        );
      })}
      <p className={desc}>{t("onboarding.appsIgnored")}</p>
    </>
  );
}

// Plain string for now; it moves to a t() key with i18n.
const CALENDAR_LATER = "later in settings › connections";
const CALENDAR_CONNECTED = "connected in settings › connections";

function ExtrasStep({ settings: s, update }: StepProps) {
  const t = useT();
  const rows: [string, string, ReactNode][] = [
    [
      t("onboarding.calendarTitle"),
      t("onboarding.calendarDesc"),
      // Windows reads the calendar's iCal address, which is pasted (and disconnected) in Settings: no toggle here.
      onWindows ? (
        s.calendarConnected ? (
          <Chip key="c" tone="gold">
            <Check size={11} aria-hidden="true" /> {CALENDAR_CONNECTED}
          </Chip>
        ) : (
          <Label key="c">{CALENDAR_LATER}</Label>
        )
      ) : (
        <CalendarSwitch key="c" settings={s} update={update} />
      ),
    ],
    [t("onboarding.browserTitle"), t("onboarding.browserDesc"), <Switch key="b" aria-label={t("onboarding.browserTitle")} checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />],
    [
      t("onboarding.notionTitle"),
      t("onboarding.notionDesc"),
      s.notion.connected ? (
        <Chip key="n" tone="gold">
          <Check size={11} aria-hidden="true" /> {t("onboarding.connected")}
        </Chip>
      ) : (
        <Label key="n">{t("onboarding.notionLater")}</Label>
      ),
    ],
  ];
  return (
    <>
      <h1 className={h1}>{t("onboarding.extrasTitle")}</h1>
      <p className={desc}>{t("onboarding.extrasDesc")}</p>
      {rows.map(([title, d, control]) => (
        <div key={title} className="flex items-center gap-3 rounded-card border border-line bg-surface px-[18px] py-4 shadow-card">
          <span className="flex-1">
            <span className="block font-semibold">{title}</span>
            <span className="text-xs text-subtle">{d}</span>
          </span>
          {control}
        </div>
      ))}
    </>
  );
}

function Done({ systemAudio }: { systemAudio: Perm }) {
  const t = useT();
  const platform = usePlatform();
  return (
    <>
      <h1 className={h1}>{t("onboarding.doneTitle")}</h1>
      <p className={desc}>{t("onboarding.doneDesc", { tray: platform.trayName })}</p>
      <div className="flex items-center gap-3.5 rounded-card bg-sp1 px-[18px] py-4">
        <Headphones size={22} aria-hidden="true" className="flex-none text-sp1-fg" />
        <div>
          <div className="font-semibold text-sp1-fg">{t("onboarding.tipTitle")}</div>
          <div className="mt-0.5 text-xs leading-[17px] text-subtle">{t("onboarding.tipDesc")}</div>
        </div>
      </div>
      {systemAudio === "denied" && (
        <p className={desc}>{t("onboarding.doneReminder", { app: platform.settingsApp })}</p>
      )}
    </>
  );
}

function CalendarSwitch({ settings: s, update }: StepProps) {
  const t = useT();
  const platform = usePlatform();
  const store = useSettingsStore();
  const toast = useToast();
  return (
    <Switch
      aria-label={t("onboarding.calendarTitle")}
      checked={s.calendarConnected}
      onChange={async (on) => {
        if (!on) return void update({ calendarConnected: false });
        if ((await store.requestCalendar()) === "allowed") void update({ calendarConnected: true });
        else toast(t("onboarding.calendarOff", { app: platform.settingsApp }));
      }}
    />
  );
}
