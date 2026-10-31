import { useCallback, useEffect, useState } from "react";
import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import { byokModels } from "../../data/byokModels";
import { hasByokKey, type ByokProvider, type LocalModel, type SummaryEngineKind } from "../../data/settings";
import { useLang, useT } from "../../lib/i18n/I18nProvider";
import { messageOr, type MessageKey, type T } from "../../lib/i18n";
import { deletedMessage } from "../../lib/modelDelete";
import { Button, Chip, Field, Label, Select, Sheet, Switch, cn, useToast } from "../../ui";
import { ChevronDown } from "lucide-react";
import { settingLanguageView } from "../../lib/summaryLanguages";
import { LanguagePicker } from "../LanguagePicker";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";
import { TEMPLATES, isTemplate, templateName } from "../../lib/templates";

/** The longest custom instructions Rust uses (summarizer/prompt.rs MAX_INSTRUCTIONS). */
const MAX_INSTRUCTIONS = 1000;

type Engine = { id: SummaryEngineKind; name: MessageKey; tag: MessageKey; note: MessageKey; costs?: boolean };

/** The two choices most people pick between, each saying what's private and what may cost money. */
const MAIN_ENGINES: Engine[] = [
  { id: "local", name: "settings.engine_local", tag: "settings.engine_local_tag", note: "settings.engine_local_note" },
  { id: "byok", name: "settings.engine_byok", tag: "settings.engine_byok_tag", note: "settings.engine_byok_note", costs: true },
];
/** A local server (Ollama, LM Studio), under Advanced; shown while it's the one in use. */
const ADVANCED_ENGINE: Engine = { id: "ollama", name: "settings.engine_ollama", tag: "settings.engine_ollama_tag", note: "settings.engine_ollama_note" };

function EngineCard({ engine: e, on, onPick }: { engine: Engine; on: boolean; onPick: () => void }) {
  const t = useT();
  return (
    <button
      type="button"
      role="radio"
      aria-checked={on}
      onClick={onPick}
      className={cn(
        "flex cursor-pointer flex-col gap-1.5 rounded-card border p-3 text-left font-[inherit] text-ink",
        on ? "border-gold bg-gold-soft shadow-[var(--t-shadow)]" : "border-line bg-surface hover:border-edge",
      )}
    >
      <span className="flex flex-wrap items-center gap-2">
        <span className="font-bold">{t(e.name)}</span>
        <Chip tone={e.costs ? "default" : "gold"}>{t(e.tag)}</Chip>
      </span>
      <span className="text-xs leading-[17px] text-subtle">{t(e.note)}</span>
    </button>
  );
}

const FORMAT: [string, MessageKey, MessageKey][] = [
  ["01", "settings.format_recap", "settings.format_recap_note"],
  ["02", "settings.format_decisions", "settings.format_decisions_note"],
  ["03", "settings.format_actions", "settings.format_actions_note"],
  ["04", "settings.format_questions", "settings.format_questions_note"],
];

export function SummaryAITab({ settings: s, update }: TabProps) {
  const t = useT();
  const [showAdvanced, setAdvanced] = useState(false);
  const advancedOpen = showAdvanced || s.summaryEngine === ADVANCED_ENGINE.id;
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro
          title={t("settings.engine_title")}
          desc={t("settings.engine_desc")}
        />
        <div className="flex flex-col gap-2.5" role="radiogroup" aria-label={t("settings.engine_title")}>
          <div className="grid grid-cols-1 gap-2.5 min-[600px]:grid-cols-2">
            {MAIN_ENGINES.map((e) => (
              <EngineCard key={e.id} engine={e} on={s.summaryEngine === e.id} onPick={() => update({ summaryEngine: e.id })} />
            ))}
          </div>
          {advancedOpen && <EngineCard engine={ADVANCED_ENGINE} on={s.summaryEngine === ADVANCED_ENGINE.id} onPick={() => update({ summaryEngine: ADVANCED_ENGINE.id })} />}
        </div>
        {s.summaryEngine !== ADVANCED_ENGINE.id && (
          <Button variant="ghost" className="-ml-2 self-start underline underline-offset-2" aria-expanded={advancedOpen} onClick={() => setAdvanced(!advancedOpen)}>
            {advancedOpen ? t("settings.engine_advanced_hide") : t("settings.engine_advanced")}
          </Button>
        )}

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
            <SummaryLanguageSetting value={s.summaryLanguage} localModel={s.summaryEngine === "local" ? s.localModel : undefined} onChange={(code) => update({ summaryLanguage: code })} />
          </Row>
          <Row title={t("settings.default_type")} desc={t("settings.default_type_desc")}>
            <Select
              label={t("settings.default_type")}
              wrapperClassName="w-52"
              value={isTemplate(s.summaryTemplate) ? s.summaryTemplate : "general"}
              onChange={(e) => update({ summaryTemplate: e.target.value })}
            >
              {TEMPLATES.map((id) => (
                <option key={id} value={id}>
                  {t(templateName(id))}
                </option>
              ))}
            </Select>
          </Row>
          <Instructions value={s.summaryInstructions ?? ""} onSave={(v) => update({ summaryInstructions: v })} />
        </Group>
      </div>
    </div>
  );
}

/** Custom instructions, saved when the box loses focus (not on every key). */
function Instructions({ value, onSave }: { value: string; onSave: (v: string) => void }) {
  const t = useT();
  const [draft, setDraft] = useState<string>();
  return (
    <Row stack title={t("settings.instructions_title")} desc={t("settings.instructions_desc")}>
      <textarea
        aria-label={t("settings.instructions_title")}
        rows={3}
        maxLength={MAX_INSTRUCTIONS}
        value={draft ?? value}
        placeholder={t("settings.instructions_placeholder")}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          if (draft !== undefined && draft.trim() !== value.trim()) onSave(draft.trim());
          setDraft(undefined);
        }}
        className="min-h-[68px] resize-y rounded-control border border-input bg-paper px-3 py-2 font-[inherit] text-[13px] leading-[19px] text-ink outline-none placeholder:text-faint focus:border-edge"
      />
    </Row>
  );
}

const gb = (mb: number) => `${(mb / 1000).toFixed(1)} GB`;

/** One line in the Local model list: the name (with Recommended on the one Talaan suggests), the download size,
 *  the memory it needs and its licence. */
export function localOption(t: T, m: Pick<LocalModel, "name" | "sizeMb" | "memoryGb" | "licence" | "recommended">): string {
  return t("settings.local_option", {
    name: m.recommended ? `${m.name} (${t("settings.local_recommended")})` : m.name,
    size: gb(m.sizeMb),
    memory: `${m.memoryGb.toFixed(1)} GB`,
    licence: m.licence,
  });
}

function LocalPanel({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const lang = useLang();
  const run = (p: Promise<unknown>) => p.catch((e) => toast(describeError(e)));
  const model = s.localModels.find((m) => m.id === s.localModel) ?? s.localModels[0];
  const pct = Math.round((model?.progress ?? 0) * 100);
  return (
    <Group>
      <Row title={t("settings.local_model_title")} desc={t("settings.local_model_desc")}>
        <Select label={t("settings.local_model_title")} wrapperClassName="w-96 max-w-full" value={model?.id ?? s.localModel} onChange={(e) => update({ localModel: e.target.value })}>
          {s.localModels.map((m) => (
            <option key={m.id} value={m.id}>
              {localOption(t, m)}
            </option>
          ))}
        </Select>
      </Row>
      {model && (
        <Row title={t("settings.status_title")} desc={messageOr(lang, `settings.model_note_${model.id}`, model.note)}>
          {model.state === "downloaded" && (
            <span className="flex items-center gap-2">
              <Chip tone="gold">{t("settings.installed")}</Chip>
              <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.deleteModel(model.id).then((r) => { const msg = deletedMessage(t, lang, model.name, r); if (msg) toast(msg); }))}>
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

/** Summary language: a button showing the choice, and the searchable language list in a sheet ("Same as meeting" on top). */
function SummaryLanguageSetting({ value, localModel, onChange }: { value: string; localModel?: string; onChange: (code: string) => void }) {
  const t = useT();
  const lang = useLang();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const view = settingLanguageView(value, lang, t("settings.summary_language_meeting"));
  return (
    <>
      <button
        type="button"
        aria-haspopup="dialog"
        aria-label={`${t("settings.summary_language")}: ${view.label}`}
        onClick={() => setOpen(true)}
        className="relative inline-flex h-8 w-52 max-w-full cursor-pointer items-center rounded-control border border-input bg-paper pl-2.5 pr-8 text-left font-[inherit] text-[13px] text-ink outline-none focus:border-edge"
      >
        <span className="min-w-0 flex-1 truncate">{view.label}</span>
        <ChevronDown aria-hidden="true" size={14} className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-subtle" />
      </button>
      <Sheet open={open} title={t("settings.summary_language")} onClose={close}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("settings.summary_language")}</h2>
          <LanguagePicker
            value={view.current}
            extra={view.extra}
            localModel={localModel}
            onPick={(code) => {
              close();
              onChange(code);
            }}
          />
          <div className="flex justify-end">
            <Button onClick={close}>{t("common.cancel")}</Button>
          </div>
        </div>
      </Sheet>
    </>
  );
}
