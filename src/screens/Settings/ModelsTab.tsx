import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import type { SpeechModel } from "../../data/settings";
import { useLang, useT } from "../../lib/i18n/I18nProvider";
import { messageOr, type MessageKey } from "../../lib/i18n";
import { Button, Chip, Radio, Select, Switch, cn, useToast } from "../../ui";
import { Dots, Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

const SOURCE = {
  on_this_mac: "settings.source_on_this_mac",
  downloaded: "settings.source_downloaded",
  downloading: "settings.source_downloading",
  not_downloaded: "settings.source_not_downloaded",
} as const satisfies Record<SpeechModel["state"], MessageKey>;

const size = (mb: number) => `${(mb / 1000).toFixed(1)} GB`;
const installed = (m: SpeechModel) => m.state === "on_this_mac" || m.state === "downloaded";

export function ModelsTab({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const lang = useLang();
  const run = (p: Promise<void>) => p.catch((e) => toast(describeError(e)));
  const reused = s.speechModels.filter((m) => m.state === "on_this_mac");
  const reuseDesc = reused.length
    ? t("settings.reuse_found", { count: reused.length, size: size(reused.reduce((a, m) => a + m.sizeMb, 0)) })
    : t("settings.reuse_none");

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro title={t("settings.model_section")} desc={t("settings.model_section_desc")} />
        <div className="flex flex-col gap-2.5" role="radiogroup" aria-label={t("settings.model_section")}>
          {s.speechModels.map((m) => {
            const on = s.activeSpeechModel === m.id;
            const pct = Math.round((m.progress ?? 0) * 100);
            const minutes = Math.max(1, Math.ceil(((1 - (m.progress ?? 0)) * m.sizeMb) / 250));
            return (
              <div
                key={m.id}
                className={cn(
                  "flex flex-wrap items-center gap-x-4 gap-y-3 rounded-card border px-3.5 py-3",
                  on ? "border-line bg-surface shadow-[inset_3px_0_0_var(--t-gold),var(--t-shadow)]" : "border-line bg-surface hover:border-edge",
                )}
              >
                <button
                  type="button"
                  role="radio"
                  aria-checked={on}
                  aria-label={t("settings.model_use", { name: m.name })}
                  onClick={() => (installed(m) ? update({ activeSpeechModel: m.id }) : toast(t("settings.model_download_first", { name: m.name })))}
                  className="cursor-pointer border-0 bg-transparent p-0"
                >
                  <Radio checked={on} className="size-5" />
                </button>
                <div className="flex min-w-0 flex-1 basis-52 flex-col gap-0.5">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-bold">{m.name}</span>
                    <span className="text-xs text-subtle">{m.maker}</span>
                    <Chip tone={installed(m) ? "gold" : "default"}>{t(SOURCE[m.state])}</Chip>
                  </div>
                  <span className="text-xs text-subtle">{messageOr(lang, `settings.model_note_${m.id}`, m.note)}</span>
                </div>
                <div className="flex flex-col gap-1">
                  <Dots n={m.speed} label={t("settings.dots_speed")} />
                  <Dots n={m.accuracy} label={t("settings.dots_accuracy")} />
                </div>
                <div className="flex min-w-24 flex-col items-end gap-1">
                  <span className="font-mono text-[11px] text-subtle">{size(m.sizeMb)}</span>
                  {m.state === "not_downloaded" && <Button onClick={() => run(store.downloadModel(m.id))}>{t("settings.download")}</Button>}
                  {m.state === "downloading" && (
                    <span className="flex w-28 flex-col items-end gap-1">
                      <span className="h-[5px] w-full overflow-hidden rounded-full bg-line">
                        <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
                      </span>
                      <span className="font-mono text-[11px] text-subtle">
                        {t("settings.model_progress", { pct, minutes })}
                      </span>
                      <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.cancelModelDownload(m.id))}>
                        {t("settings.cancel")}
                      </button>
                    </span>
                  )}
                  {m.state === "downloaded" && !on && (
                    <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.deleteModel(m.id))}>
                      {t("settings.delete")}
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <Group>
        <ExtraModelRow
          model={s.speakerModel}
          title={t("settings.split_title")}
          desc={t("settings.split_desc", { maker: s.speakerModel.maker })}
          on={s.separateSpeakers}
          onToggle={(v) => update({ separateSpeakers: v })}
        />
        <ExtraModelRow
          model={s.searchModel}
          title={t("settings.search_title")}
          desc={t("settings.search_desc")}
        />
      </Group>

      <Group>
        <Row title={t("settings.reuse_title")} desc={reuseDesc}>
          <Switch aria-label={t("settings.reuse_title")} checked={s.reuseModels} onChange={(v) => update({ reuseModels: v })} />
        </Row>
        <Row title={t("settings.meeting_language_title")} desc={t("settings.meeting_language_desc")}>
          <Select label={t("settings.meeting_language_title")} wrapperClassName="w-40" value={s.meetingLanguage} onChange={(e) => update({ meetingLanguage: e.target.value })}>
            <option value="auto">{t("settings.lang_auto")}</option>
            <option value="en">{t("settings.lang_en")}</option>
            <option value="tl">{t("settings.lang_tl")}</option>
            <option value="es">{t("settings.lang_es")}</option>
            <option value="fr">{t("settings.lang_fr")}</option>
            <option value="de">{t("settings.lang_de")}</option>
            <option value="pt">{t("settings.lang_pt")}</option>
          </Select>
        </Row>
        <Row title={t("settings.gpu_title")} desc={t("settings.gpu_desc")}>
          <Switch aria-label={t("settings.gpu_title")} checked={s.useGpu} onChange={(v) => update({ useGpu: v })} />
        </Row>
      </Group>
    </div>
  );
}

/** An add-on model (speaker separation, search): its own row with download, cancel, delete and an optional switch. */
function ExtraModelRow({ model: m, title, desc, on, onToggle }: { model: SpeechModel; title: string; desc: string; on?: boolean; onToggle?: (v: boolean) => void }) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const run = (p: Promise<void>) => p.catch((e) => toast(describeError(e)));
  const pct = Math.round((m.progress ?? 0) * 100);
  return (
    <Row title={title} desc={desc}>
      {m.state === "not_downloaded" && <Button onClick={() => run(store.downloadModel(m.id))}>{t("settings.model_download_mb", { size: m.sizeMb })}</Button>}
      {m.state === "downloading" && (
        <span className="flex w-28 flex-col items-end gap-1">
          <span className="h-[5px] w-full overflow-hidden rounded-full bg-line">
            <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
          </span>
          <span className="flex gap-2 font-mono text-[11px] text-subtle">
            {pct}%
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.cancelModelDownload(m.id))}>
              {t("settings.cancel")}
            </button>
          </span>
        </span>
      )}
      {installed(m) && (
        <span className="flex items-center gap-3">
          {m.state === "downloaded" && (
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.deleteModel(m.id))}>
              {t("settings.delete")}
            </button>
          )}
          {onToggle ? <Switch aria-label={title} checked={!!on} onChange={onToggle} /> : <Chip tone="gold">{t("settings.on")}</Chip>}
        </span>
      )}
    </Row>
  );
}
