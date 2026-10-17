import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Headphones, MessageSquareText, ShieldCheck } from "lucide-react";
import { Logo } from "../../brand/Logo";
import { recorderClient } from "../../data";
import { useSettings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import type { Settings } from "../../data/settings";
import { Button, Chip, Field, Label, Radio, Switch, cn, useToast } from "../../ui";
import { SunCorner } from "../../brand/SunCorner";
import { onWindows, systemAudioNeedsPermission, usePlatform } from "../../lib/platform";
import { openSettings } from "../../lib/windows";
import { useT } from "../../lib/i18n/I18nProvider";
import { LeavesTable } from "../Settings/LeavesTable";
import { setupItems } from "../../lib/setupDownload";
import { DownloadAnnouncer, SetupProgress, gb, useSetupDownload } from "./SetupDownload";

/** The steps, in order. "audio" is left out on Windows when the microphone is already allowed (nothing to do);
 *  "speech" and "summary" only show after "Choose individually". */
type StepId = "welcome" | "audio" | "setup" | "speech" | "summary" | "apps" | "extras" | "done";

const STEP_NAME: Record<StepId, "step0" | "step1" | "stepSetup" | "step2" | "step3" | "step4" | "step5" | "step6"> = {
  welcome: "step0",
  audio: "step1",
  setup: "stepSetup",
  speech: "step2",
  summary: "step3",
  apps: "step4",
  extras: "step5",
  done: "step6",
};

/**
 * "asked": the system prompt was shown; its answer can't be read, only seen in the first recording.
 * "not_needed": the system has no permission for it (system audio on Windows).
 */
type Perm = "idle" | "allowed" | "asked" | "denied" | "not_needed";

const micState = (p: string): Perm => (p === "granted" ? "allowed" : p === "denied" || p === "restricted" ? "denied" : "idle");

export function OnboardingScreen() {
  const t = useT();
  const { settings, update } = useSettings();
  const [at, setAt] = useState<StepId>("welcome");
  const [systemAudio, setSystemAudio] = useState<Perm>(systemAudioNeedsPermission ? "idle" : "not_needed");
  const [mic, setMic] = useState<Perm>("idle");
  // Windows: a microphone that was allowed before onboarding started means the audio step has nothing to ask.
  const [micAllowedAtStart, setMicAllowedAtStart] = useState(false);
  // Decided when leaving Welcome, so a late answer can't remove the step the person is on.
  const [skipAudio, setSkipAudio] = useState(false);
  const [custom, setCustom] = useState(false);
  const [name, setName] = useState<string>();
  const [connectionsAfter, setConnectionsAfter] = useState<string[]>([]);

  useEffect(() => {
    recorderClient.microphonePermission().then(
      (p) => {
        setMic(micState(p));
        setMicAllowedAtStart(p === "granted");
      },
      () => {},
    );
  }, []);

  if (!settings) return null;

  const steps: StepId[] = [
    "welcome",
    ...(skipAudio ? [] : (["audio"] as const)),
    "setup",
    ...(custom ? (["speech", "summary"] as const) : []),
    "apps",
    "extras",
    "done",
  ];
  const index = at === "welcome" ? 0 : Math.max(1, steps.indexOf(at));
  const last = at === "done";

  const finish = async () => {
    await update({
      onboardingDone: true,
      // The one-time "find Talaan in the system tray" hint (Windows hides new tray icons).
      ...(onWindows ? { showTrayTip: true } : {}),
      ...(name !== undefined ? { yourName: name.trim() } : {}),
    });
    if (connectionsAfter.length > 0) void openSettings("connections");
  };
  const go = (to: StepId | undefined) => to && setAt(to);
  const next = () => (last ? void finish() : go(steps[index + 1]));

  const welcome = at === "welcome";
  const compact = welcome || at === "done";

  return (
    <div className="flex h-full items-center justify-center overflow-hidden bg-paper p-6 [@media(max-height:680px)]:p-3">
      <div
        className={cn(
          "relative flex max-h-full w-[800px] max-w-full flex-col overflow-hidden rounded-[18px] border border-line bg-surface shadow-[0_16px_40px_rgba(17,19,24,0.12)]",
          !compact && "h-[620px]",
        )}
      >
        <SunCorner size={welcome ? 200 : 170} className="absolute right-0 top-0" />
        {welcome ? (
          <>
            <div className="relative flex min-h-0 flex-1 flex-col items-center gap-4 overflow-auto px-16 pb-2 pt-10 text-center [@media(max-height:680px)]:pt-6">
              <Welcome />
            </div>
            <div className="relative flex justify-center pb-7 pt-4">
              <Button
                variant="primary"
                className="h-[42px] px-6 text-sm"
                onClick={() => {
                  const skip = onWindows && micAllowedAtStart;
                  setSkipAudio(skip);
                  setAt(skip ? "setup" : "audio");
                }}
              >
                {t("onboarding.getStarted")}
              </Button>
            </div>
          </>
        ) : (
          <>
            <div className="relative flex items-center gap-2.5 px-7 py-[18px] [@media(max-height:680px)]:py-3">
              <Logo size={28} />
              <span className="font-display text-[17px] font-bold tracking-[-0.02em]">talaan</span>
              <span className="flex-1" />
              <ol className="m-0 mr-24 flex list-none gap-1.5 p-0" aria-label={t("onboarding.setupSteps")}>
                {steps.slice(1).map((id, k) => (
                  <li
                    key={id}
                    aria-label={t(`onboarding.${STEP_NAME[id]}`)}
                    aria-current={id === at ? "step" : undefined}
                    className={cn("h-1.5 rounded-full transition-[width]", id === at ? "w-[22px]" : "w-1.5", k + 1 <= index ? "bg-gold" : "bg-edge")}
                  />
                ))}
              </ol>
            </div>
            {at === "audio" && <AudioAccess mic={mic} setMic={setMic} systemAudio={systemAudio} setSystemAudio={setSystemAudio} />}
            {at === "setup" && (
              <SetupStep
                settings={settings}
                onChooseIndividually={() => {
                  setCustom(true);
                  setAt("speech");
                }}
              />
            )}
            {at === "speech" && <ModelStep settings={settings} update={update} />}
            {at === "summary" && <SummaryStep settings={settings} update={update} />}
            {at === "apps" && <AppsStep settings={settings} update={update} />}
            {at === "extras" && <ExtrasStep settings={settings} update={update} connectionsAfter={connectionsAfter} setConnectionsAfter={setConnectionsAfter} />}
            {at === "done" && <Done systemAudio={systemAudio} settings={settings} name={name} setName={setName} />}
            <div className="relative flex items-center gap-2 border-t border-line px-7 py-[16px] [@media(max-height:680px)]:py-3">
              <Label>{t("onboarding.stepOf", { n: index, total: steps.length - 1, name: t(`onboarding.${STEP_NAME[at]}`) })}</Label>
              <span className="flex-1" />
              <Button variant="ghost" onClick={() => go(steps[index - 1])}>
                {t("onboarding.back")}
              </Button>
              <Button variant="primary" onClick={next}>
                {last ? t("onboarding.openTalaan") : skippable(at, settings) ? t("onboarding.skipForNow") : t("onboarding.continue")}
              </Button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

const ready = (state: string) => state === "downloaded" || state === "on_this_mac";

/** The step's Continue reads "Skip for now" while what it offers isn't downloaded (or downloading). */
function skippable(at: StepId, s: Settings): boolean {
  if (at === "setup") {
    const { speech, summary } = setupItems(s);
    return [speech, summary].every((m) => !m || m.state === "not_downloaded");
  }
  if (at === "speech") return s.speechModels.find((m) => m.id === s.activeSpeechModel)?.state === "not_downloaded";
  if (at === "summary") return s.summaryEngine === "local" && s.localModels.find((m) => m.choice === (s.localModel ?? "recommended"))?.state === "not_downloaded";
  return false;
}

const h1 = "m-0 font-display text-[30px] font-bold leading-[34px] tracking-[-0.02em] [@media(max-height:680px)]:text-[26px] [@media(max-height:680px)]:leading-[30px]";
const desc = "m-0 max-w-[560px] text-sm leading-[21px] text-subtle";

/** A step's scrolling content, and an optional strip pinned above the footer (the download button, so it's always
 *  in view at the smallest window size). */
function StepBody({ children, pinned }: { children: ReactNode; pinned?: ReactNode }) {
  return (
    <>
      <div className="relative flex min-h-0 flex-1 flex-col gap-[22px] overflow-auto px-16 pb-6 pt-2 [@media(max-height:680px)]:gap-4 [@media(max-height:680px)]:pb-4">{children}</div>
      {pinned && <div className="relative border-t border-line bg-surface px-16 py-3.5">{pinned}</div>}
    </>
  );
}

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
        <Logo size={52} />
        <span className="font-display text-[42px] font-bold leading-none tracking-[-0.02em]">talaan</span>
      </div>
      <p className="m-0 max-w-[520px] text-balance text-[13px] leading-[19px] text-subtle">{t("onboarding.welcomeOrigin")}</p>
      <h1 className="m-0 max-w-[560px] font-display text-[38px] font-bold leading-[42px] tracking-[-0.02em] [@media(max-height:680px)]:text-[32px] [@media(max-height:680px)]:leading-[36px]">
        {t("onboarding.welcomeTitle")}
      </h1>
      <p className="m-0 max-w-[480px] text-[15px] leading-[23px] text-subtle">{t("onboarding.welcomeDesc")}</p>
      <div className="flex max-w-[540px] flex-col gap-2 rounded-card bg-sunken px-4 py-3 text-left text-[13px] leading-[19px]">
        <span className="flex gap-2.5">
          <ShieldCheck size={16} aria-hidden="true" className="mt-px flex-none text-gold-ink" />
          {t("onboarding.welcomePrivacy")}
        </span>
        <span className="flex gap-2.5">
          <MessageSquareText size={16} aria-hidden="true" className="mt-px flex-none text-gold-ink" />
          {t("onboarding.welcomeConsent")}
        </span>
      </div>
      <div className="flex flex-wrap justify-center gap-2">
        <Chip>{t("onboarding.chipNoBot")}</Chip>
        <Chip>{t("onboarding.chipOffline")}</Chip>
        <Chip>{t("onboarding.chipAsks")}</Chip>
      </div>
    </>
  );
}

function PermRow({ title, desc: d, state, onAllow, onDeny, onOpen, deniedNote }: { title: string; desc: string; state: Perm; onAllow?: () => void; onDeny?: () => void; onOpen?: () => void; deniedNote?: string }) {
  const t = useT();
  const platform = usePlatform();
  return (
    <div className="flex flex-col gap-2 rounded-card border border-line bg-surface px-[18px] py-4 shadow-card">
      <div className="flex items-center gap-3.5">
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
      {state === "denied" && deniedNote && (
        <p role="alert" className="m-0 text-[13px] leading-[19px] text-warn-fg">
          {deniedNote}
        </p>
      )}
    </div>
  );
}

function AudioAccess({ mic, setMic, systemAudio, setSystemAudio }: { mic: Perm; setMic: (p: Perm) => void; systemAudio: Perm; setSystemAudio: (p: Perm) => void }) {
  const t = useT();
  const platform = usePlatform();
  // Read again when the step opens and whenever the window comes back (the person may have changed it in the
  // system settings meanwhile).
  useEffect(() => {
    const read = () => void recorderClient.microphonePermission().then((p) => setMic(micState(p)), () => {});
    read();
    window.addEventListener("focus", read);
    return () => window.removeEventListener("focus", read);
  }, []);
  const askMic = async () => setMic((await recorderClient.requestMicrophone()) ? "allowed" : "denied");
  const askSystem = async () => setSystemAudio((await recorderClient.requestSystemAudio()) ? "asked" : "denied");
  const micRow = (
    <PermRow
      title={t("onboarding.micTitle")}
      desc={t("onboarding.micDesc")}
      state={mic}
      onAllow={() => void askMic()}
      onOpen={() => void recorderClient.openSettings("microphone")}
      deniedNote={t("onboarding.micDenied")}
    />
  );
  // Windows: system audio needs no permission, so this step is the microphone alone (and the intro says so).
  if (!systemAudioNeedsPermission) {
    return (
      <StepBody>
        <h1 className={h1}>{t("onboarding.audioTitleMic")}</h1>
        <p className={desc}>{platform.audioIntro}</p>
        {micRow}
      </StepBody>
    );
  }
  return (
    <StepBody>
      <h1 className={h1}>{t("onboarding.audioTitle")}</h1>
      <p className={desc}>{platform.audioIntro}</p>
      {micRow}
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
    </StepBody>
  );
}

/** "Get Talaan ready": one button for the speech model and the summary model, or "Choose individually". */
function SetupStep({ settings: s, onChooseIndividually }: { settings: Settings; onChooseIndividually: () => void }) {
  const t = useT();
  const setup = useSetupDownload(s);
  const { speech, summary, view } = setup;
  const allReady = setup.missingMb === 0;
  const failed = !view.active && view.failed.length > 0;
  // Focus follows the button that replaced the one just pressed (Download → Cancel, Try again → Cancel).
  const cancelRef = useRef<HTMLButtonElement>(null);
  const downloadRef = useRef<HTMLButtonElement>(null);
  const pressed = useRef<"start" | "cancel" | undefined>(undefined);
  useEffect(() => {
    if (pressed.current === "start" && view.active) cancelRef.current?.focus();
    if (pressed.current === "cancel" && !view.active) downloadRef.current?.focus();
    pressed.current = undefined;
  }, [view.active]);

  const row = (label: string, item: typeof speech) =>
    item && (
      <div key={label} className="flex items-center gap-3 border-t border-line py-2.5 first:border-t-0">
        <span className="flex-1">
          <span className="block text-[13px] font-semibold">{label}</span>
          <span className="text-xs text-faint">{item.name}</span>
        </span>
        {ready(item.state) ? (
          <Chip tone="gold">
            <Check size={11} aria-hidden="true" /> {t("onboarding.setupItemReady")}
          </Chip>
        ) : item.state === "downloading" || (item.partial ?? 0) > 0 ? (
          <span className="font-mono text-xs text-subtle">{Math.floor(((item.state === "downloading" ? item.progress : item.partial) ?? 0) * 100)}%</span>
        ) : (
          <span className="font-mono text-xs text-subtle">{gb(item.sizeMb)} GB</span>
        )}
      </div>
    );

  return (
    <StepBody>
      <DownloadAnnouncer view={view} />
      <h1 className={h1}>{t("onboarding.setupTitle")}</h1>
      <p className={desc}>{t(summary ? "onboarding.setupDesc" : s.summaryEngine === "ollama" ? "onboarding.setupDescSpeechServer" : "onboarding.setupDescSpeech")}</p>
      <div className="flex flex-col rounded-card border border-line bg-surface px-[18px] py-1.5 shadow-card">
        {row(t("onboarding.setupSpeech"), speech)}
        {row(t("onboarding.setupSummary"), summary)}
      </div>
      {allReady ? (
        <p className="m-0 text-sm font-semibold">{t("onboarding.setupReady")}</p>
      ) : view.active || failed ? (
        <div className="flex flex-col gap-3">
          <SetupProgress fraction={view.fraction} minutesLeft={view.minutesLeft} failed={failed} />
          {failed ? (
            <div className="flex flex-wrap items-center gap-3 rounded-card bg-warn-bg px-3 py-2.5 text-[13px] text-warn-fg">
              <span className="flex-1 basis-56">{view.failed[0].error}</span>
              <Button
                ref={downloadRef}
                onClick={() => {
                  pressed.current = "start";
                  void setup.retry();
                }}
              >
                {t("onboarding.setupRetry")}
              </Button>
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-3">
              <span className="flex-1 basis-56 text-[13px] text-subtle">{t("onboarding.setupKeepUsing")}</span>
              <Button
                ref={cancelRef}
                variant="ghost"
                onClick={() => {
                  pressed.current = "cancel";
                  void setup.cancel();
                }}
              >
                {t("onboarding.cancelDownload")}
              </Button>
            </div>
          )}
        </div>
      ) : (
        <div className="flex flex-col items-start gap-2.5">
          <Button
            ref={downloadRef}
            variant="primary"
            size="lg"
            onClick={() => {
              pressed.current = "start";
              void setup.start();
            }}
          >
            {setup.resumable ? t("onboarding.setupButtonRest", { size: gb(setup.missingMb) }) : t("onboarding.setupButtonNoTime", { size: gb(setup.missingMb) })}
          </Button>
          <span className="text-[13px] text-subtle">
            {t("onboarding.setupKeepUsing")} {t("onboarding.notYet")}
          </span>
        </div>
      )}
      <button type="button" className="cursor-pointer self-start border-0 bg-transparent p-0 text-[13px] font-semibold text-ink underline" onClick={onChooseIndividually}>
        {t("onboarding.chooseIndividually")}
      </button>
    </StepBody>
  );
}

type StepProps = { settings: Settings; update: (p: Partial<Settings>) => Promise<Settings> };

/** How many speech models setup lists before "Show all" (the catalog lists the recommended ones first). */
const FEATURED_MODELS = 3;

function ModelStep({ settings: s, update }: StepProps) {
  const t = useT();
  const store = useSettingsStore();
  const status = (m: Settings["speechModels"][number]) => {
    if (m.state === "on_this_mac") return t("onboarding.modelOnThisComputer");
    if (m.state === "downloaded") return t("onboarding.modelDownloaded");
    if (m.state === "downloading") return t("onboarding.modelDownloading", { pct: Math.round((m.progress ?? 0) * 100) });
    return t("onboarding.modelSize", { size: gb(m.sizeMb) });
  };
  const chosen = s.speechModels.find((m) => m.id === s.activeSpeechModel);
  // The three recommended models first; the rest behind "Show all" (the chosen one always shows).
  const [all, setAll] = useState(false);
  const shown = all ? s.speechModels : s.speechModels.filter((m, i) => i < FEATURED_MODELS || m.id === s.activeSpeechModel);
  return (
    <StepBody pinned={chosen && <DownloadRow item={chosen} onDownload={() => void store.downloadModel(chosen.id)} onCancel={() => void store.cancelModelDownload(chosen.id)} />}>
      <h1 className={h1}>{t("onboarding.modelTitle")}</h1>
      <p className={desc}>{t("onboarding.modelDesc")}</p>
      <div role="radiogroup" aria-label={t("onboarding.modelGroup")} className="flex flex-col gap-2">
        {shown.map((m) => (
          <Option
            key={m.id}
            on={s.activeSpeechModel === m.id}
            title={m.name}
            tag={m.id === "whisper-turbo" ? t("onboarding.recommended") : undefined}
            onClick={() => update({ activeSpeechModel: m.id })}
          >
            {m.note} {status(m)}
          </Option>
        ))}
      </div>
      {!all && s.speechModels.length > shown.length && (
        <button type="button" className="cursor-pointer self-start border-0 bg-transparent p-0 text-[13px] font-semibold text-ink underline" onClick={() => setAll(true)}>
          {t("onboarding.showAllModels", { count: s.speechModels.length })}
        </button>
      )}
    </StepBody>
  );
}

/** The chosen model's download: nothing downloads until the person clicks, and Continue never waits for it. */
function DownloadRow({ item, onDownload, onCancel, missingNote }: { item: { name: string; state: string; progress?: number; sizeMb: number; error?: string }; onDownload: () => void; onCancel: () => void; missingNote?: string }) {
  const t = useT();
  const pct = Math.round((item.progress ?? 0) * 100);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const downloadRef = useRef<HTMLButtonElement>(null);
  const pressed = useRef(false);
  useEffect(() => {
    if (!pressed.current) return;
    pressed.current = false;
    (item.state === "downloading" ? cancelRef : downloadRef).current?.focus();
  }, [item.state]);
  const announce = (
    <DownloadAnnouncer
      view={{
        active: item.state === "downloading",
        failed: item.state === "not_downloaded" && item.error ? [{ id: "", name: item.name, sizeMb: item.sizeMb, state: item.state, error: item.error }] : [],
        fraction: item.progress ?? 0,
        items: [],
      }}
    />
  );
  if (ready(item.state)) {
    return (
      <div className="flex items-center gap-2 text-[13px] font-semibold">
        {announce}
        <Check size={15} aria-hidden="true" className="text-gold-ink" />
        {t("onboarding.readyName", { name: item.name })}
      </div>
    );
  }
  if (item.state === "downloading") {
    return (
      <div className="flex items-center gap-2.5">
        {announce}
        <span aria-hidden="true" className="h-1.5 flex-1 overflow-hidden rounded-full bg-line">
          <span className="block h-full bg-gold" style={{ width: `${pct}%` }} />
        </span>
        <Label>{t("onboarding.localDownloading", { pct })}</Label>
        <Button
          ref={cancelRef}
          variant="ghost"
          onClick={() => {
            pressed.current = true;
            onCancel();
          }}
        >
          {t("onboarding.cancelDownload")}
        </Button>
      </div>
    );
  }
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
      {announce}
      <Button
        ref={downloadRef}
        variant="primary"
        onClick={() => {
          pressed.current = true;
          onDownload();
        }}
      >
        {item.error ? t("onboarding.setupRetry") : t("onboarding.downloadName", { name: item.name, size: gb(item.sizeMb) })}
      </Button>
      <span className="flex-1 basis-56 text-[13px] leading-[18px] text-subtle">
        {item.error ? (
          <strong className="font-semibold text-warn-fg">{item.error} </strong>
        ) : missingNote ? (
          <strong className="font-semibold text-ink">{missingNote} </strong>
        ) : null}
        {t("onboarding.notYet")}
      </span>
    </div>
  );
}

function SummaryStep({ settings: s, update }: StepProps) {
  const t = useT();
  const store = useSettingsStore();
  const local = s.summaryEngine === "local";
  const model = s.localModels.find((m) => m.choice === (s.localModel ?? "recommended")) ?? s.localModels[0];

  return (
    <StepBody
      pinned={
        local &&
        model && (
          <DownloadRow
            item={model}
            onDownload={() => void store.downloadModel(model.id)}
            onCancel={() => void store.cancelModelDownload(model.id)}
            missingNote={t("onboarding.summaryNoDownload")}
          />
        )
      }
    >
      <h1 className={h1}>{t("onboarding.summaryTitle")}</h1>
      <p className={desc}>{t("onboarding.summaryDesc")}</p>
      <div role="radiogroup" aria-label={t("onboarding.summaryGroup")} className="flex flex-col gap-2">
        <Option on={local} title={t("onboarding.localTitle")} tag={t("onboarding.defaultTag")} onClick={() => update({ summaryEngine: "local" })}>
          {model ? t("onboarding.localDesc", { size: gb(model.sizeMb) }) : t("onboarding.localDescNoSize")}
        </Option>
        <Option on={s.summaryEngine === "byok"} title={t("onboarding.byokTitle")} onClick={() => update({ summaryEngine: "byok" })}>
          {t("onboarding.byokDesc")}
        </Option>
      </div>
      <div className="rounded-card bg-sunken px-4 py-3">
        <LeavesTable settings={s} compact />
      </div>
    </StepBody>
  );
}

function AppsStep({ settings: s, update }: StepProps) {
  const t = useT();
  const setAlways = (id: string, always: boolean) =>
    update({ callApps: s.callApps.map((a) => (a.id === id ? { ...a, rule: always ? "always" : "ask" } : a)) });
  return (
    <StepBody>
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
      <p className={desc}>{t(onWindows ? "onboarding.appsIgnoredWindows" : "onboarding.appsIgnored")}</p>
    </StepBody>
  );
}

function ExtrasStep({ settings: s, update, connectionsAfter, setConnectionsAfter }: StepProps & { connectionsAfter: string[]; setConnectionsAfter: (keys: string[]) => void }) {
  const t = useT();
  const connected = (key: string, label: string) => (
    <Chip key={key} tone="gold">
      <Check size={11} aria-hidden="true" /> {label}
    </Chip>
  );
  // Calendar (Windows) and Notion are set up in Settings › Connections, which opens when onboarding ends. Focus
  // moves to whichever control replaced the one pressed.
  const toFocus = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (toFocus.current) document.getElementById(`setup-${toFocus.current}`)?.focus();
    toFocus.current = undefined;
  }, [connectionsAfter]);
  const setUp = (key: string, name: string) =>
    connectionsAfter.includes(key) ? (
      <Chip
        key={key}
        id={`setup-${key}`}
        tone="gold"
        aria-pressed={true}
        aria-label={t("onboarding.setUpUndo", { name })}
        onClick={() => {
          toFocus.current = key;
          setConnectionsAfter(connectionsAfter.filter((k) => k !== key));
        }}
      >
        <Check size={11} aria-hidden="true" /> {t("onboarding.setUpAfter")}
      </Chip>
    ) : (
      <Button
        key={key}
        id={`setup-${key}`}
        aria-label={t("onboarding.setUpRow", { name })}
        onClick={() => {
          toFocus.current = key;
          setConnectionsAfter([...connectionsAfter, key]);
        }}
      >
        {t("onboarding.setUp")}
      </Button>
    );
  const rows: [string, string, ReactNode][] = [
    [
      t("onboarding.calendarTitle"),
      t("onboarding.calendarDesc"),
      // Windows reads the calendar's iCal address, which is pasted (and disconnected) in Settings: no toggle here.
      onWindows ? (s.calendarConnected ? connected("c", t("onboarding.calendarConnected")) : setUp("c", t("onboarding.calendarTitle"))) : <CalendarSwitch key="c" settings={s} update={update} />,
    ],
    [t("onboarding.browserTitle"), t("onboarding.browserDesc"), <Switch key="b" aria-label={t("onboarding.browserTitle")} checked={s.tabDetection} onChange={(v) => update({ tabDetection: v })} />],
    [t("onboarding.notionTitle"), t("onboarding.notionDesc"), s.notion.connected ? connected("n", t("onboarding.connected")) : setUp("n", t("onboarding.notionTitle"))],
  ];
  return (
    <StepBody>
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
    </StepBody>
  );
}

function Done({ systemAudio, settings: s, name, setName }: { systemAudio: Perm; settings: Settings; name?: string; setName: (n: string | undefined) => void }) {
  const t = useT();
  const platform = usePlatform();
  const [skipped, setSkipped] = useState(false);
  const speech = s.speechModels.find((m) => m.id === s.activeSpeechModel);
  const local = s.localModels.find((m) => m.choice === (s.localModel ?? "recommended"));
  const noSpeech = !s.speechModels.some((m) => m.state !== "not_downloaded");
  const downloading = speech?.state === "downloading" || (s.summaryEngine === "local" && local?.state === "downloading");
  const noSummary = s.summaryEngine === "local" && local?.state === "not_downloaded";
  const noKey = s.summaryEngine === "byok" && s.byokKeys.length === 0;
  const warn = "m-0 rounded-card bg-warn-bg px-3.5 py-2.5 text-[13px] leading-[19px] text-warn-fg";
  return (
    <div className="relative flex min-h-0 flex-1 flex-col gap-4 overflow-auto px-16 pb-6 pt-2">
      <h1 className={h1}>{t("onboarding.doneTitle")}</h1>
      <p className={desc}>{t("onboarding.doneDesc", { tray: platform.trayName })}</p>
      {!skipped && (
        <div className="flex flex-col gap-2 rounded-card border border-line bg-surface px-[18px] py-3.5 shadow-card">
          <span aria-hidden="true" className="font-semibold">
            {t("onboarding.nameQuestion")}
          </span>
          <div className="flex items-center gap-2">
            <Field
              label={t("onboarding.nameQuestion")}
              wrapperClassName="w-64"
              placeholder={t("onboarding.namePlaceholder")}
              value={name ?? s.yourName}
              maxLength={60}
              autoComplete="given-name"
              onChange={(e) => setName(e.target.value)}
            />
            <Button
              variant="ghost"
              onClick={() => {
                setName(undefined);
                setSkipped(true);
              }}
            >
              {t("onboarding.nameSkip")}
            </Button>
          </div>
          <span className="text-xs text-subtle">{t("onboarding.nameHint")}</span>
        </div>
      )}
      <div className="flex items-center gap-3.5 rounded-card bg-sp1 px-[18px] py-3.5">
        <Headphones size={22} aria-hidden="true" className="flex-none text-sp1-fg" />
        <div>
          <div className="font-semibold text-sp1-fg">{t("onboarding.tipTitle")}</div>
          <div className="mt-0.5 text-xs leading-[17px] text-subtle">{t("onboarding.tipDesc")}</div>
        </div>
      </div>
      {systemAudio === "denied" && <p className={warn}>{t("onboarding.doneReminder", { app: platform.settingsApp })}</p>}
      {downloading ? (
        <p className={desc}>{t("onboarding.doneDownloading")}</p>
      ) : (
        <>
          {noSpeech && <p className={warn}>{t("onboarding.doneNoModel")}</p>}
          {noSummary && <p className={warn}>{t("onboarding.doneNoSummary")}</p>}
        </>
      )}
      {noKey && <p className={warn}>{t("onboarding.doneNoKey")}</p>}
    </div>
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
