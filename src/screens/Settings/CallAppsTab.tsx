import { X } from "lucide-react";
import type { CallRule } from "../../data/settings";
import { useLang, useT } from "../../lib/i18n/I18nProvider";
import { messageOr } from "../../lib/i18n";
import { Switch, cn } from "../../ui";
import { Group, InlineAdd, Row, SectionIntro, Segmented } from "./parts";
import type { TabProps } from "./tabProps";

/** App badge tints, cycling pastel blue, gold and red. */
const TINTS = ["bg-sp1 text-sp1-fg", "bg-sp3 text-sp3-fg", "bg-sp2 text-sp2-fg"];

const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-");

/** The stored English note is data: the known ones are translated for display. */
const ADDED_BY_YOU = "Added by you";

export function CallAppsTab({ settings: s, update }: TabProps) {
  const t = useT();
  const lang = useLang();
  const noteOf = (a: { id: string; note: string }) =>
    a.note === ADDED_BY_YOU ? t("settings.app_note_added_by_you") : messageOr(lang, `settings.app_note_${a.id}`, a.note);
  const whyOf = (why: string) => messageOr(lang, `settings.why_${why.replace(/ /g, "_")}`, why);
  const setRule = (id: string, rule: CallRule) => update({ callApps: s.callApps.map((a) => (a.id === id ? { ...a, rule } : a)) });

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro
          title={t("settings.apps_title")}
          desc={t("settings.apps_desc")}
        />
        <Group>
          {s.callApps.map((a, i) => {
            const lockAlways = a.isBrowser && !s.tabDetection;
            return (
              <div key={a.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 py-4">
                <span aria-hidden="true" className={cn("grid size-[34px] flex-none place-items-center rounded-[10px] text-sm font-bold", TINTS[i % TINTS.length])}>
                  {a.name.charAt(0)}
                </span>
                <div className="min-w-0 flex-1 basis-48">
                  <div className="text-[13px] font-semibold">{a.name}</div>
                  <div className="text-xs leading-[17px] text-subtle">{noteOf(a)}</div>
                </div>
                {a.note === ADDED_BY_YOU && (
                  <button
                    type="button"
                    aria-label={t("settings.apps_remove", { name: a.name })}
                    onClick={() => update({ callApps: s.callApps.filter((x) => x.id !== a.id) })}
                    className="grid cursor-pointer place-items-center border-0 bg-transparent p-1 text-subtle hover:text-ink"
                  >
                    <X size={14} aria-hidden="true" />
                  </button>
                )}
                <Segmented
                  label={a.name}
                  value={lockAlways && a.rule === "always" ? "ask" : a.rule}
                  onChange={(v) => setRule(a.id, v)}
                  options={[
                    { value: "ask", label: t("settings.rule_ask") },
                    { value: "always", label: t("settings.rule_always"), disabled: lockAlways, title: lockAlways ? t("settings.rule_locked") : undefined },
                    { value: "never", label: t("settings.rule_never") },
                  ]}
                />
              </div>
            );
          })}
        </Group>
        <InlineAdd
          label={t("settings.apps_add")}
          placeholder={t("settings.apps_add_placeholder")}
          onAdd={(name) => update({ callApps: [...s.callApps, { id: `${slug(name)}-${s.callApps.length}`, name, note: ADDED_BY_YOU, rule: "ask" }] })}
        />
        <Group>
          <Row title={t("settings.tag_title")} desc={t("settings.tag_desc")}>
            <Switch aria-label={t("settings.tag_title")} checked={s.autoTagApps} onChange={(v) => update({ autoTagApps: v })} />
          </Row>
        </Group>
      </div>

      <div className="flex flex-col gap-3">
        <SectionIntro
          title={t("settings.ignored_title")}
          desc={t("settings.ignored_desc")}
        />
        <div className="flex flex-wrap gap-2">
          {s.ignoredApps.map((g) => (
            <span key={g.name} className="inline-flex h-8 items-center gap-2 rounded-full border border-line bg-surface pl-3 pr-1.5">
              <span className="text-[13px] font-semibold">{g.name}</span>
              <span className="font-mono text-[11px] lowercase text-faint">{whyOf(g.why)}</span>
              <button
                type="button"
                aria-label={t("settings.ignored_stop", { name: g.name })}
                onClick={() => update({ ignoredApps: s.ignoredApps.filter((x) => x.name !== g.name) })}
                className="grid size-[22px] cursor-pointer place-items-center rounded-full border-0 bg-rail text-subtle hover:text-ink"
              >
                <X size={12} aria-hidden="true" />
              </button>
            </span>
          ))}
        </div>
        <InlineAdd
          label={t("settings.ignored_add")}
          placeholder={t("settings.apps_add_placeholder")}
          onAdd={(name) => !s.ignoredApps.some((g) => g.name === name) && update({ ignoredApps: [...s.ignoredApps, { name, why: "added by you" }] })}
        />
      </div>
    </div>
  );
}

