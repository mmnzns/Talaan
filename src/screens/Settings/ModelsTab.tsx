import { useState } from "react";
import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import type { SpeechModel } from "../../data/settings";
import { useLang, useT } from "../../lib/i18n/I18nProvider";
import type { MessageKey } from "../../lib/i18n";
import { deletedMessage } from "../../lib/modelDelete";
import { modelNote, splitByTier, tierOf } from "../../lib/modelTiers";
import { Button, Chip, Radio, Select, Switch, cn, useToast } from "../../ui";
import { Dots, Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

const SOURCE = {
  downloaded: "settings.source_downloaded",
  downloading: "settings.source_downloading",
  not_downloaded: "settings.source_not_downloaded",
} as const satisfies Record<SpeechModel["state"], MessageKey>;

const size = (mb: number) => `${(mb / 1000).toFixed(1)} GB`;
const installed = (m: SpeechModel) => m.state === "downloaded";

export function ModelsTab({ settings: s, update }: TabProps) {
  const t = useT();
  const { tiered, more } = splitByTier(s.speechModels);
  // "More models" starts open when the model in use is one of them, so the choice is never hidden.
  const [showMore, setMore] = useState<boolean>();
  const moreOpen = showMore ?? more.some((m) => m.id === s.activeSpeechModel);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro title={t("settings.model_section")} desc={t("settings.model_section_desc")} />
        <div className="flex flex-col gap-2.5" role="radiogroup" aria-label={t("settings.model_section")}>
          {tiered.map((m) => (
            <ModelCard key={m.id} model={m} settings={s} update={update} />
          ))}
          {moreOpen && more.map((m) => <ModelCard key={m.id} model={m} settings={s} update={update} />)}
        </div>
        {more.length > 0 && (
          <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <Button variant="ghost" className="-ml-2 underline underline-offset-2" aria-expanded={moreOpen} onClick={() => setMore(!moreOpen)}>
              {moreOpen ? t("settings.more_models_hide") : t("settings.more_models_show", { count: more.length })}
            </Button>
            <span className="text-xs text-subtle">{t("settings.more_models_desc")}</span>
          </div>
        )}
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
      </Group>
    </div>
  );
}

/** One speech model: a tier name ("Best for most people") with the technical name small below it, or, under More
 *  models, the technical name itself. */
function ModelCard({ model: m, settings: s, update }: { model: SpeechModel } & TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const lang = useLang();
  const run = (p: Promise<unknown>) => p.catch((e) => toast(describeError(e)));
  const remove = () => run(store.deleteModel(m.id).then((r) => { const msg = deletedMessage(t, lang, m.name, r); if (msg) toast(msg); }));
  const on = s.activeSpeechModel === m.id;
  const tier = tierOf(m.id);
  const pct = Math.round((m.progress ?? 0) * 100);
  const minutes = Math.max(1, Math.ceil(((1 - (m.progress ?? 0)) * m.sizeMb) / 250));
  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-x-4 gap-y-3 rounded-card border px-3.5 py-3",
        on ? "border-gold bg-gold-soft shadow-[var(--t-shadow)]" : "border-line bg-surface hover:border-edge",
      )}
    >
      <button
        type="button"
        role="radio"
        aria-checked={on}
        aria-label={t("settings.model_use", { name: tier ? `${t(tier.label)} (${m.name})` : m.name })}
        onClick={() => (installed(m) ? update({ activeSpeechModel: m.id }) : toast(t("settings.model_download_first", { name: m.name })))}
        className="cursor-pointer border-0 bg-transparent p-0"
      >
        <Radio checked={on} className="size-5" />
      </button>
      <div className="flex min-w-0 flex-1 basis-52 flex-col gap-0.5">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-bold">{tier ? t(tier.label) : m.name}</span>
          {!tier && <span className="text-xs text-subtle">{m.maker}</span>}
          <Chip tone={installed(m) ? "gold" : "default"}>{t(SOURCE[m.state])}</Chip>
          {m.englishOnly && <Chip>{t("settings.english_only")}</Chip>}
        </div>
        {tier && <span className="text-xs text-subtle">{t("settings.model_tech", { name: m.name, maker: m.maker })}</span>}
        <span className="text-xs text-subtle">{modelNote(lang, m)}</span>
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
            <span className="font-mono text-[11px] text-subtle">{t("settings.model_progress", { pct, minutes })}</span>
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.cancelModelDownload(m.id))}>
              {t("settings.cancel")}
            </button>
          </span>
        )}
        {m.state === "downloaded" && !on && (
          <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={remove}>
            {t("settings.delete")}
          </button>
        )}
      </div>
    </div>
  );
}

/** An add-on model (speaker separation, search): its own row with download, cancel, delete and an optional switch. */
function ExtraModelRow({ model: m, title, desc, on, onToggle }: { model: SpeechModel; title: string; desc: string; on?: boolean; onToggle?: (v: boolean) => void }) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const lang = useLang();
  const run = (p: Promise<unknown>) => p.catch((e) => toast(describeError(e)));
  const remove = () => run(store.deleteModel(m.id).then((r) => { const msg = deletedMessage(t, lang, title, r); if (msg) toast(msg); }));
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
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={remove}>
              {t("settings.delete")}
            </button>
          )}
          {onToggle ? <Switch aria-label={title} checked={!!on} onChange={onToggle} /> : <Chip tone="gold">{t("settings.on")}</Chip>}
        </span>
      )}
    </Row>
  );
}
