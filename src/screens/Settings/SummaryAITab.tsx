import { useEffect, useState } from "react";
import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import { byokModels } from "../../data/byokModels";
import { hasByokKey, type ByokProvider, type SummaryEngineKind } from "../../data/settings";
import { useLang, useT } from "../../lib/i18n/I18nProvider";
import { messageOr, type MessageKey } from "../../lib/i18n";
import { Button, Chip, Field, Label, Select, Switch, cn, useToast } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

const ENGINES: { id: SummaryEngineKind; name: MessageKey; tag: MessageKey; note: MessageKey }[] = [
  { id: "local", name: "settings.engine_local", tag: "settings.engine_local_tag", note: "settings.engine_local_note" },
  { id: "ollama", name: "settings.engine_ollama", tag: "settings.engine_ollama_tag", note: "settings.engine_ollama_note" },
  { id: "byok", name: "settings.engine_byok", tag: "settings.engine_byok_tag", note: "settings.engine_byok_note" },
];

const FORMAT: [string, MessageKey, MessageKey][] = [
  ["01", "settings.format_recap", "settings.format_recap_note"],
  ["02", "settings.format_decisions", "settings.format_decisions_note"],
  ["03", "settings.format_actions", "settings.format_actions_note"],
  ["04", "settings.format_questions", "settings.format_questions_note"],
];

export function SummaryAITab({ settings: s, update }: TabProps) {
  const t = useT();
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro
          title={t("settings.engine_title")}
          desc={t("settings.engine_desc")}
        />
        <div className="grid grid-cols-1 gap-2.5 min-[620px]:grid-cols-3" role="radiogroup" aria-label={t("settings.engine_title")}>
          {ENGINES.map((e) => {
            const on = s.summaryEngine === e.id;
            return (
              <button
                key={e.id}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => update({ summaryEngine: e.id })}
                className={cn(
                  "flex cursor-pointer flex-col gap-1.5 rounded-card border p-3 text-left font-[inherit] text-ink",
                  on ? "border-line bg-surface shadow-[inset_3px_0_0_var(--t-gold),var(--t-shadow)]" : "border-line bg-surface hover:border-edge",
                )}
              >
                <span className="flex items-center gap-2">
                  <span className="font-bold">{t(e.name)}</span>
                  <Chip tone={e.id === "byok" ? "default" : "gold"}>{t(e.tag)}</Chip>
                </span>
                <span className="text-xs leading-[17px] text-subtle">{t(e.note)}</span>
              </button>
            );
          })}
        </div>

        {s.summaryEngine === "local" && <LocalPanel {...{ settings: s, update }} />}
        {s.summaryEngine === "ollama" && <OllamaPanel {...{ settings: s, update }} />}
        {s.summaryEngine === "byok" && <ByokPanel {...{ settings: s, update }} />}
      </div>

      <div className="flex flex-col gap-3">
        <div className="flex items-baseline gap-3">
          <SectionIntro title={t("settings.format_title")} />
          <Label>{t("settings.format_same")}</Label>
        </div>
        <Group>
          <div className="grid grid-cols-2 divide-x divide-line min-[620px]:grid-cols-4">
            {FORMAT.map(([n, name, note]) => (
              <div key={n} className="flex flex-col gap-0.5 px-4 py-3">
                <span className="font-mono text-[11px] text-subtle">{n}</span>
                <span className="text-[13px] font-bold">{t(name)}</span>
                <span className="text-xs text-subtle">{t(note)}</span>
              </div>
            ))}
          </div>
          <Row title={t("settings.summarize_auto")} desc={t("settings.summarize_auto_desc")}>
            <Switch aria-label={t("settings.summarize_auto")} checked={s.summarizeAutomatically} onChange={(v) => update({ summarizeAutomatically: v })} />
          </Row>
          <Row title={t("settings.summary_language")}>
            <Select label={t("settings.summary_language")} wrapperClassName="w-44" value={s.summaryLanguage} onChange={(e) => update({ summaryLanguage: e.target.value })}>
              <option value="meeting">{t("settings.summary_language_meeting")}</option>
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
    </div>
  );
}

const gb = (mb: number) => `${(mb / 1000).toFixed(1)} GB`;

function LocalPanel({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const lang = useLang();
  const run = (p: Promise<void>) => p.catch((e) => toast(describeError(e)));
  const model = s.localModels.find((m) => m.choice === s.localModel) ?? s.localModels[0];
  const pct = Math.round((model?.progress ?? 0) * 100);
  return (
    <Group>
      <Row title={t("settings.local_model_title")} desc={t("settings.local_model_desc")}>
        <Select label={t("settings.local_model_title")} wrapperClassName="w-60" value={s.localModel} onChange={(e) => update({ localModel: e.target.value as "recommended" | "larger" })}>
          {s.localModels.map((m) => (
            <option key={m.id} value={m.choice}>
              {t("settings.local_option", { choice: m.choice === "recommended" ? t("settings.local_recommended") : t("settings.local_larger"), name: m.name, size: gb(m.sizeMb) })}
            </option>
          ))}
        </Select>
      </Row>
      {model && (
        <Row title={t("settings.status_title")} desc={messageOr(lang, `settings.model_note_${model.id}`, model.note)}>
          {model.state === "downloaded" && (
            <span className="flex items-center gap-2">
              <Chip tone="gold">{t("settings.installed")}</Chip>
              <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.deleteModel(model.id))}>
                {t("settings.delete")}
              </button>
            </span>
          )}
          {model.state === "not_downloaded" && <Button onClick={() => run(store.downloadModel(model.id))}>{t("settings.download_size", { size: gb(model.sizeMb) })}</Button>}
          {model.state === "downloading" && (
            <span className="flex w-36 flex-col items-end gap-1">
              <span className="h-[5px] w-full overflow-hidden rounded-full bg-line">
                <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
              </span>
              <span className="font-mono text-[11px] text-subtle">{pct}%</span>
              <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.cancelModelDownload(model.id))}>
                {t("settings.cancel")}
              </button>
            </span>
          )}
        </Row>
      )}
    </Group>
  );
}

function OllamaPanel({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const t = useT();
  const [models, setModels] = useState<string[]>();
  const [problem, setProblem] = useState<string>();

  useEffect(() => {
    let live = true;
    const timer = setTimeout(() => {
      store
        .localServerModels(s.ollamaAddress)
        .then((m) => live && (setModels(m), setProblem(undefined)))
        .catch((e) => live && (setModels(undefined), setProblem(describeError(e))));
    }, 400);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [store, s.ollamaAddress]);

  return (
    <Group>
      <Row title={t("settings.ollama_title")} desc={t("settings.ollama_desc")}>
        <Field label={t("settings.ollama_address")} value={s.ollamaAddress} onChange={(e) => update({ ollamaAddress: e.target.value })} wrapperClassName="w-60" />
        {problem && (
          <div role="status" className="basis-full text-xs font-semibold text-danger">
            {problem}
          </div>
        )}
      </Row>
      <Row title={t("settings.ollama_model_title")} desc={models ? t("settings.ollama_models_found", { count: models.length }) : t("settings.ollama_models_unknown")}>
        <Select label={t("settings.ollama_model_title")} wrapperClassName="w-52" value={s.ollamaModel ?? ""} onChange={(e) => update({ ollamaModel: e.target.value || undefined })}>
          <option value="">{t("settings.ollama_first")}</option>
          {s.ollamaModel && !models?.includes(s.ollamaModel) && <option value={s.ollamaModel}>{s.ollamaModel}</option>}
          {models?.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </Select>
      </Row>
    </Group>
  );
}

/** Provider ids with their menu names: a message key, or a brand name that needs no translation. */
const PROVIDERS: [ByokProvider, string][] = [
  ["claude", "settings.provider_claude"],
  ["openai", "settings.provider_openai"],
  ["gemini", "settings.provider_gemini"],
  ["groq", "Groq"],
  ["cerebras", "Cerebras"],
  ["mistral", "Mistral"],
  ["openrouter", "OpenRouter"],
  ["compatible", "settings.provider_compatible"],
];

function ByokPanel({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const t = useT();
  const [key, setKey] = useState("");
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; message: string }>();
  const models = byokModels(s.byokProvider);
  const listed = s.byokModel === "recommended" || models.some((m) => m.id === s.byokModel);
  // A model id typed by hand: for a model newer than this list, or any model on your own server.
  const [typing, setTyping] = useState(!listed);
  const custom = typing || !listed || models.length === 0;
  const keySaved = hasByokKey(s);

  const test = async () => {
    setTesting(true);
    setResult(undefined);
    try {
      const r = await store.testApiKey(s.byokProvider, key);
      setResult(r);
      if (r.ok) setKey("");
    } catch (e) {
      setResult({ ok: false, message: describeError(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <Group>
      <Row title={t("settings.byok_provider")} desc={t("settings.byok_provider_desc")}>
        <Select label={t("settings.byok_provider")} wrapperClassName="w-52" value={s.byokProvider} onChange={(e) => (setResult(undefined), setTyping(false), update({ byokProvider: e.target.value as ByokProvider, byokModel: "recommended" }))}>
          {PROVIDERS.map(([id, name]) => {
            const label = name.startsWith("settings.") ? t(name as MessageKey) : name;
            return (
              <option key={id} value={id}>
                {s.byokKeys.includes(id) ? t("settings.provider_with_key", { name: label }) : label}
              </option>
            );
          })}
        </Select>
      </Row>
      {s.byokProvider === "compatible" && (
        <Row title={t("settings.byok_server")} desc={t("settings.byok_server_desc")}>
          <Field label={t("settings.byok_server")} value={s.byokBaseUrl ?? ""} onChange={(e) => update({ byokBaseUrl: e.target.value })} wrapperClassName="w-60" />
        </Row>
      )}
      <Row title={t("settings.byok_key")} desc={keySaved ? t("settings.byok_key_saved") : t("settings.byok_key_new")}>
        <div className="flex items-center gap-2">
          <Field label={t("settings.byok_key")} type="password" placeholder={keySaved ? "••••••••••••" : t("settings.byok_key_placeholder")} value={key} onChange={(e) => setKey(e.target.value)} wrapperClassName="w-52" />
          <Button onClick={test} disabled={testing || !key.trim()}>
            {testing ? t("settings.byok_testing") : t("settings.byok_test")}
          </Button>
          {keySaved && (
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => void store.clearApiKey(s.byokProvider).then(() => setResult(undefined))}>
              {t("settings.remove")}
            </button>
          )}
        </div>
        {result && (
          <div role="status" className={cn("basis-full text-xs font-semibold", result.ok ? "text-ok" : "text-danger")}>
            {result.message}
          </div>
        )}
      </Row>
      <Row title={t("settings.byok_model")} desc={models.length ? t("settings.byok_model_desc") : t("settings.byok_model_desc_server")}>
        <div className="flex flex-col items-end gap-2">
          {models.length > 0 && (
            <Select
              label={t("settings.byok_model")}
              wrapperClassName="w-72"
              value={custom ? "__custom" : s.byokModel}
              onChange={(e) => {
                const v = e.target.value;
                setTyping(v === "__custom");
                if (v !== "__custom") update({ byokModel: v });
              }}
            >
              <option value="recommended">{t("settings.byok_model_recommended", { name: models[0].name })}</option>
              {models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))}
              <option value="__custom">{t("settings.byok_model_typed")}</option>
            </Select>
          )}
          {custom && (
            <Field
              label={t("settings.byok_model_id")}
              placeholder={models.length ? t("settings.byok_model_id_placeholder") : t("settings.byok_model_placeholder")}
              value={s.byokModel === "recommended" ? "" : s.byokModel}
              onChange={(e) => update({ byokModel: e.target.value.trim() || "recommended" })}
              wrapperClassName="w-72"
            />
          )}
        </div>
      </Row>
    </Group>
  );
}
