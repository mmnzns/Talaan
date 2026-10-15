import { X } from "lucide-react";
import type { CallRule } from "../../data/settings";
import { Switch } from "../../ui";
import { Group, InlineAdd, Row, SectionIntro, Segmented } from "./parts";
import type { TabProps } from "./tabProps";

const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-");

export function CallAppsTab({ settings: s, update }: TabProps) {
  const setRule = (id: string, rule: CallRule) => update({ callApps: s.callApps.map((a) => (a.id === id ? { ...a, rule } : a)) });

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro
          title="Call apps"
          desc="What Talaan does when one of these starts using your mic. Ask is the default. Always record still shows a banner with the recording notice and Stop."
        />
        <Group>
          {s.callApps.map((a) => {
            const lockAlways = a.isBrowser && !s.tabDetection;
            return (
              <div key={a.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3">
                <span aria-hidden="true" className="grid size-8 flex-none place-items-center rounded-control border border-edge bg-sunken text-sm font-extrabold">
                  {a.name.charAt(0)}
                </span>
                <div className="min-w-0 flex-1 basis-48">
                  <div className="text-[13px] font-semibold">{a.name}</div>
                  <div className="text-xs leading-[17px] text-subtle">{a.note}</div>
                </div>
                {a.note === "Added by you" && (
                  <button
                    type="button"
                    aria-label={`Remove ${a.name}`}
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
                    { value: "ask", label: "Ask" },
                    { value: "always", label: "Always", disabled: lockAlways, title: lockAlways ? "Turn on browser call detection in Connections first" : undefined },
                    { value: "never", label: "Never" },
                  ]}
                />
              </div>
            );
          })}
        </Group>
        <InlineAdd
          label="+ Add an app"
          placeholder="App name"
          onAdd={(name) => update({ callApps: [...s.callApps, { id: `${slug(name)}-${s.callApps.length}`, name, note: "Added by you", rule: "ask" }] })}
        />
        <Group>
          <Row title="Tag meetings by call app" desc="A call from Zoom gets #zoom, from Slack #slack, and so on. Meetings you start yourself aren't tagged.">
            <Switch aria-label="Tag meetings by call app" checked={s.autoTagApps} onChange={(v) => update({ autoTagApps: v })} />
          </Row>
        </Group>
      </div>

      <div className="flex flex-col gap-3">
        <SectionIntro
          title="Ignored apps"
          desc="Dictation and voice apps also use the mic. Talaan never prompts or records for these. Common ones are added automatically when they're installed."
        />
        <div className="flex flex-wrap gap-2">
          {s.ignoredApps.map((g) => (
            <span key={g.name} className="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface py-[3px] pl-2.5 pr-1.5 text-xs">
              {g.name}
              <span className="font-mono text-[10px] text-subtle">{g.why}</span>
              <button
                type="button"
                aria-label={`Stop ignoring ${g.name}`}
                onClick={() => update({ ignoredApps: s.ignoredApps.filter((x) => x.name !== g.name) })}
                className="grid cursor-pointer place-items-center border-0 bg-transparent p-0.5 text-subtle hover:text-ink"
              >
                <X size={12} aria-hidden="true" />
              </button>
            </span>
          ))}
        </div>
        <InlineAdd
          label="+ Ignore an app"
          placeholder="App name"
          onAdd={(name) => !s.ignoredApps.some((g) => g.name === name) && update({ ignoredApps: [...s.ignoredApps, { name, why: "added by you" }] })}
        />
      </div>
    </div>
  );
}

