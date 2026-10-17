import { AlertTriangle, Check, ChevronDown, Pause, Play, Square, Star } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useMeeting, useSettings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import type { RecorderLevels, RecorderNotice, RecorderState } from "../../data/recorder";
import { formatTime } from "../../lib/format";
import { copyText } from "../../lib/share";
import { Button, Chip, Label, Switch, cn } from "../../ui";
import { inTauri } from "../../lib/windows";
import { SunCorner } from "../../brand/SunCorner";
import { usePlatform } from "../../lib/platform";
import { useT } from "../../lib/i18n/I18nProvider";

interface Props {
  state: RecorderState;
  levels: RecorderLevels;
  notice?: RecorderNotice;
  app?: string;
  /** CSS max height; by default the screen (in the app's own window) or the viewport (in a page). */
  maxHeight?: string;
  onPause: () => void;
  onResume: () => void;
  onMark: (note?: string) => void;
  onStop: () => void;
  onDiscard: () => void;
  onOpenSettings?: (pane: NonNullable<RecorderNotice["settingsPane"]>) => void;
  onDismissNotice?: () => void;
}

const BARS = 10;

/** Turns one level (0..1) into a row of bars that moves like a meter. */
function useBars(level: number, active: boolean, frame: unknown) {
  const history = useRef<number[]>(Array(BARS).fill(0));
  const [bars, setBars] = useState<number[]>(history.current);
  useEffect(() => {
    history.current = active ? [level, ...history.current.slice(0, BARS - 1)] : Array(BARS).fill(0);
    setBars([...history.current]);
    // `frame` changes with every levels event, so a steady level (e.g. silence) still scrolls the meter.
  }, [level, active, frame]);
  return bars;
}

function Bars({ levels, tone }: { levels: number[]; tone: "me" | "them" }) {
  return (
    <div aria-hidden="true" className="flex h-[22px] items-center gap-[3px]">
      {levels.map((l, i) => (
        <span
          key={i}
          className={cn("w-1 rounded-sm transition-[height] duration-150", l > 0.05 ? (tone === "me" ? "bg-ink" : "bg-gold") : "bg-line")}
          style={{ height: l > 0.05 ? 5 + Math.round(l * 17) : 3 }}
        />
      ))}
    </div>
  );
}

/** "Transcript only" for this recording: on and off, from the meeting on disk (the Screens stage has none). */
function useTranscriptOnly(meetingId?: string): [boolean, (on: boolean) => void] {
  const store = useStore();
  const { meeting } = useMeeting(meetingId || undefined);
  const [preview, setPreview] = useState(false);
  const on = meetingId ? Boolean(meeting?.transcriptOnly) : preview;
  return [on, (v) => (meetingId ? void store.setTranscriptOnly(meetingId, v) : setPreview(v))];
}

/** Transcript only: it waits for Generate summary instead of going to the summary engine. */
function TranscriptOnly({ on, onChange }: { on: boolean; onChange: (on: boolean) => void }) {
  const t = useT();
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-[13px] font-bold">{t("recording.transcriptOnly")}</span>
        <span className="text-xs leading-[17px] text-subtle">{t("recording.transcriptOnlyDesc")}</span>
      </span>
      <Switch aria-label={t("recording.transcriptOnly")} checked={on} onChange={onChange} />
    </label>
  );
}

/** Room left above and below the popover: the menu bar or taskbar gap, and the window's shadow. */
const SCREEN_MARGIN = 24;

/**
 * In the app the popover is its own window sized to fit, so its limit is the screen (on Windows the tray window
 * passes the exact room above the taskbar as `maxHeight`); in a page, the viewport.
 */
function defaultMaxHeight(): string {
  const avail = typeof window !== "undefined" && inTauri() ? window.screen?.availHeight : undefined;
  return avail ? `${Math.max(320, avail - SCREEN_MARGIN)}px` : "calc(100vh - 76px)";
}

/**
 * Menu bar popover while recording: notice, levels, pause, mark, quick note, stop. Compact so it fits a 768 px
 * screen: the notice folds away once it's copied, Transcript only sits behind More, the middle scrolls when it
 * still doesn't fit, and Stop and save stays in view at the bottom.
 */
export function RecordingPopover({ state, levels, notice, app, maxHeight, onPause, onResume, onMark, onStop, onDiscard, onOpenSettings, onDismissNotice }: Props) {
  const t = useT();
  const platform = usePlatform();
  const appName = app ?? t("recording.appDefault");
  const { settings, update } = useSettings();
  const [copied, setCopied] = useState(false);
  const [showNotice, setShowNotice] = useState(true);
  const [more, setMore] = useState(false);
  const [note, setNote] = useState("");
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [transcriptOnly, setTranscriptOnly] = useTranscriptOnly(state.meetingId);
  const moreId = useId();
  // Copy notice folds the box away and Show notice opens it: keep the keyboard on the button that's there now.
  const showNoticeButton = useRef<HTMLButtonElement>(null);
  const copyButton = useRef<HTMLButtonElement>(null);
  const moveFocus = useRef(false);
  useEffect(() => {
    if (!moveFocus.current) return;
    moveFocus.current = false;
    (showNotice ? copyButton : showNoticeButton).current?.focus();
  }, [showNotice]);
  const paused = state.state === "paused";
  const me = useBars(levels.me, !paused, levels);
  const them = useBars(levels.them, !paused, levels);

  // A new recording starts with the notice open again.
  useEffect(() => {
    setCopied(false);
    setShowNotice(true);
  }, [state.meetingId]);

  const text = settings?.recordingNotice ?? "";
  const info = [
    paused ? t("recording.infoPaused") : null,
    t("recording.marks", { count: state.marks }),
    t("recording.notes", { count: state.notes }),
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div
      role="dialog"
      aria-label={t("recording.recordingAria")}
      style={{ maxHeight: maxHeight ?? defaultMaxHeight() }}
      className="flex w-[356px] flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-[0_16px_40px_rgba(17,19,24,0.16)]"
    >
      <div className="relative flex flex-none flex-col gap-1 overflow-hidden border-b border-line px-[22px] pb-3.5 pt-[18px]">
        <SunCorner size={104} className="absolute right-0 top-0" />
        <div className="relative flex items-center gap-2">
          <span className={cn("size-[9px] rounded-full", paused ? "bg-faint" : "animate-pulse bg-red shadow-[0_0_0_4px_color-mix(in_srgb,var(--t-red)_20%,transparent)]")} />
          <Label>
            {paused ? t("recording.paused") : t("recording.recordingState")} · {appName}
          </Label>
        </div>
        <span className="relative font-mono text-[34px] font-medium leading-10 tracking-[-0.03em]" aria-label={t("recording.elapsedAria")}>
          {formatTime(state.elapsedMs)}
        </span>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-[18px] overflow-y-auto px-[22px] py-[18px]">
        {copied && !showNotice ? (
          <div className="flex items-center gap-2 rounded-[10px] bg-gold-soft px-3 py-2 text-gold-ink">
            <Check size={14} aria-hidden="true" className="flex-none" />
            <span className="min-w-0 flex-1 text-xs font-semibold leading-[17px]">{t("recording.noticeCopied")}</span>
            <Button
              ref={showNoticeButton}
              variant="ghost"
              className="h-7 px-2 text-xs text-gold-ink hover:bg-surface"
              onClick={() => {
                moveFocus.current = true;
                setShowNotice(true);
              }}
            >
              {t("recording.showNotice")}
            </Button>
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            <span className="font-bold">{t("recording.letKnow")}</span>
            <div className="rounded-[10px] bg-rail px-3 py-2 text-[13px] leading-[18px]">{text}</div>
            <Button
              ref={copyButton}
              className={cn("h-[34px]", copied && "bg-gold-soft text-gold-ink")}
              onClick={async () => {
                await copyText(text);
                setCopied(true);
                moveFocus.current = true;
                setShowNotice(false);
              }}
            >
              {copied ? t("recording.copiedPaste") : t("recording.copyNotice")}
            </Button>
          </div>
        )}

        {state.onBuiltInSpeakers && settings?.warnOnSpeakers !== false && (
          <div role="note" className="flex items-start gap-2.5 rounded-[10px] bg-warn-bg px-3 py-2.5 text-warn-fg">
            <AlertTriangle size={16} aria-hidden="true" className="mt-px flex-none" />
            <span className="text-xs leading-[17px]">{t("recording.speakersWarning")}</span>
          </div>
        )}

        {state.soundRelay && settings && settings.soundRelayNoticeSeen !== state.soundRelay && (
          <div role="note" className="flex flex-col gap-2 rounded-[10px] bg-warn-bg px-3 py-2.5 text-warn-fg">
            <span className="flex items-start gap-2.5">
              <AlertTriangle size={16} aria-hidden="true" className="mt-px flex-none" />
              <span className="text-xs leading-[17px]">{t("recording.relayNotice", { name: state.soundRelay })}</span>
            </span>
            <span className="flex">
              <Button variant="ghost" className="h-7" onClick={() => void update({ soundRelayNoticeSeen: state.soundRelay ?? undefined })}>
                {t("recording.ok")}
              </Button>
            </span>
          </div>
        )}

        {notice && (
          <div
            role="status"
            className={cn(
              "flex flex-col gap-2 rounded-control border px-3 py-2 text-xs leading-[17px]",
              notice.kind === "warning" ? "border-transparent bg-warn-bg text-warn-fg" : "border-transparent bg-rail",
            )}
          >
            <span>{notice.text}</span>
            <span className="flex gap-2">
              {notice.settingsPane && onOpenSettings && (
                <Button className="h-7" onClick={() => onOpenSettings(notice.settingsPane!)}>
                  {platform.openSettings}
                </Button>
              )}
              {onDismissNotice && (
                <Button variant="ghost" className="h-7" onClick={onDismissNotice}>
                  {t("recording.ok")}
                </Button>
              )}
            </span>
          </div>
        )}

        <div className="flex flex-col gap-2">
          <div className="grid grid-cols-[46px_minmax(0,1fr)] items-center gap-2.5">
            <span className="rounded-md bg-strong py-[3px] text-center text-[11px] font-bold text-strong-fg">{t("recording.me")}</span>
            <Bars levels={me} tone="me" />
          </div>
          <div className="grid grid-cols-[46px_minmax(0,1fr)] items-center gap-2.5">
            <span className="rounded-md bg-gold-soft py-[3px] text-center text-[11px] font-bold text-gold-ink">{t("recording.them")}</span>
            <Bars levels={them} tone="them" />
          </div>
          <Label>
            {t("recording.micInfo", {
              mic: state.micName || t("recording.micNone"),
              audio: state.systemAudio ? t("recording.savedEvery") : t("recording.noCallAudio"),
            })}
          </Label>
        </div>

        <div className="flex flex-col gap-2">
          <div className="flex gap-2">
            <Button size="lg" variant={paused ? "strong" : "default"} onClick={paused ? onResume : onPause}>
              {paused ? <Play size={13} fill="currentColor" aria-hidden="true" /> : <Pause size={13} fill="currentColor" aria-hidden="true" />}
              {paused ? t("recording.resume") : t("recording.pause")}
            </Button>
            <Button size="lg" className="flex-1" disabled={paused} onClick={() => onMark()}>
              <Star size={13} aria-hidden="true" className="fill-gold text-gold" /> {t("recording.markMoment")}
            </Button>
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (note.trim()) {
                onMark(note.trim());
                setNote("");
              }
            }}
          >
            <label className="flex h-[34px] items-center gap-2 rounded-control border border-input bg-paper px-3 focus-within:border-ink">
              <span className="sr-only">{t("recording.quickNoteLabel")}</span>
              <input
                value={note}
                onChange={(e) => setNote(e.target.value)}
                disabled={paused}
                placeholder={t("recording.quickNotePlaceholder")}
                className="min-w-0 flex-1 border-0 bg-transparent font-[inherit] text-[13px] text-ink outline-none placeholder:text-faint"
              />
            </label>
          </form>
          <Label>{info}</Label>
        </div>

        <div className="flex flex-col gap-3">
          <span className="flex items-center gap-2">
            <button
              type="button"
              aria-expanded={more}
              aria-controls={moreId}
              onClick={() => setMore((m) => !m)}
              className="-mx-1 flex cursor-pointer items-center gap-1.5 rounded-md border-0 bg-transparent px-1 py-0.5 font-[inherit] text-xs font-semibold text-subtle outline-offset-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-ink"
            >
              <ChevronDown size={13} aria-hidden="true" className={cn("transition-transform", more && "rotate-180")} />
              {more ? t("recording.less") : t("recording.more")}
            </button>
            {/* Says Transcript only is on while it's folded away. */}
            {!more && transcriptOnly && <Chip tone="gold">{t("recording.transcriptOnly")}</Chip>}
          </span>
          <div id={moreId} hidden={!more}>
            <TranscriptOnly on={transcriptOnly} onChange={setTranscriptOnly} />
          </div>
        </div>
      </div>

      <div className="flex flex-none gap-2.5 border-t border-line px-[22px] py-3.5">
        <Button variant="primary" className="h-[42px] flex-1 text-sm" onClick={onStop}>
          <Square size={12} fill="currentColor" aria-hidden="true" className="text-red" /> {t("recording.stopSave")}
        </Button>
        <Button
          variant="ghost"
          className="h-[42px] text-warn-fg hover:bg-warn-bg"
          onClick={() => (confirmDiscard ? onDiscard() : setConfirmDiscard(true))}
          onBlur={() => setConfirmDiscard(false)}
        >
          {confirmDiscard ? t("recording.reallyDiscard") : t("recording.discard")}
        </Button>
      </div>
    </div>
  );
}
