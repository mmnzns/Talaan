import { useEffect, useState } from "react";
import { useMeetings } from "../../data/hooks";
import { describeError } from "../../data/errors";
import { useSettingsStore, useStore } from "../../data/StoreProvider";
import { exportAll } from "../../lib/exportAll";
import type { RetentionChoice, TranscriptRetention } from "../../data/settings";
import { formatBytes, tildePath } from "../../lib/format";
import { onWindows } from "../../lib/platform";
import { useT } from "../../lib/i18n/I18nProvider";
import type { MessageKey } from "../../lib/i18n";
import { Button, Label, Select, Sheet, useToast } from "../../ui";
import { Group, Row } from "./parts";
import type { TabProps } from "./tabProps";

export function PrivacyTab({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const { meetings } = useMeetings();
  const [confirm, setConfirm] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [exporting, setExporting] = useState(false);
  const meetingStore = useStore();
  const [info, setInfo] = useState<{ path: string; bytes: number }>();

  useEffect(() => {
    store.dataInfo().then(setInfo, () => setInfo(undefined));
  }, [store, meetings.length]);

  const flows: [MessageKey, MessageKey][] = [
    ["settings.flow_audio", "settings.when_never"],
    ["settings.flow_transcripts", "settings.when_never"],
    ["settings.flow_byok", s.summaryEngine === "byok" ? "settings.when_on" : "settings.when_off"],
    ["settings.flow_notion", s.notion.connected ? "settings.when_export" : "settings.when_not_connected"],
    ["settings.flow_update", s.checkUpdates ? "settings.when_launch" : "settings.when_off"],
    ...(onWindows && s.calendarConnected ? [["settings.flow_calendar_windows", "settings.when_recording_start"] as [MessageKey, MessageKey]] : []),
    ["settings.flow_log", "settings.when_you_send"],
    ["settings.flow_analytics", "settings.when_never_collected"],
  ];

  return (
    <div className="flex flex-col gap-3">
      <Group>
        <div className="flex flex-col gap-2.5 py-4">
          <Label>{t("settings.leaves_label")}</Label>
          <div className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1.5 text-[13px]">
            {flows.map(([what, when]) => (
              <div key={what} className="contents">
                <span>{t(what)}</span>
                <span className={when === "settings.when_on" ? "font-bold text-danger" : "font-mono text-xs text-subtle"}>{t(when)}</span>
              </div>
            ))}
          </div>
        </div>
        <Row title={t("settings.audio_title")} desc={t("settings.audio_desc")}>
          <Select label={t("settings.audio_title")} wrapperClassName="w-40" value={s.retention} onChange={(e) => update({ retention: e.target.value as RetentionChoice })}>
            <option value="1d">{t("settings.keep_1d")}</option>
            <option value="7d">{t("settings.keep_7d")}</option>
            <option value="30d">{t("settings.keep_30d")}</option>
            <option value="forever">{t("settings.keep_forever")}</option>
            <option value="none">{t("settings.keep_none")}</option>
          </Select>
        </Row>
        <Row title={t("settings.transcripts_title")} desc={t("settings.transcripts_desc")}>
          <Select label={t("settings.transcripts_aria")} wrapperClassName="w-40" value={s.transcriptRetention} onChange={(e) => update({ transcriptRetention: e.target.value as TranscriptRetention })}>
            <option value="forever">{t("settings.keep_forever")}</option>
            <option value="1y">{t("settings.keep_1y")}</option>
            <option value="90d">{t("settings.keep_90d")}</option>
          </Select>
        </Row>
        <Row
          title={t("settings.folder_title")}
          desc={
            info
              ? t("settings.folder_info", { path: tildePath(info.path), size: formatBytes(info.bytes), count: meetings.length })
              : t("settings.folder_count", { count: meetings.length })
          }
        >
          <div className="flex gap-2">
            <Button onClick={() => store.revealData().catch((e) => toast(describeError(e)))}>{t("settings.folder_open")}</Button>
            <Button
              disabled={exporting || meetings.length === 0}
              onClick={async () => {
                setExporting(true);
                try {
                  const n = await exportAll(meetingStore);
                  if (n !== undefined) toast(t("settings.folder_exported", { count: n }));
                } catch (e) {
                  toast(describeError(e));
                } finally {
                  setExporting(false);
                }
              }}
            >
              {exporting ? t("settings.folder_exporting") : t("settings.folder_export")}
            </Button>
          </div>
        </Row>
        <Row title={t("settings.keys_title")} desc={t("settings.keys_desc")}>
          <Button
            disabled={s.byokKeys.length === 0}
            onClick={() =>
              void store
                .clearAllApiKeys()
                .then(() => toast(t("settings.keys_removed")))
                .catch((e) => toast(describeError(e)))
            }
          >
            {t("settings.keys_remove")}
          </Button>
        </Row>
        <Row title={t("settings.wipe_title")} desc={t("settings.wipe_desc")}>
          <Button variant="danger" onClick={() => setConfirm(true)}>
            {t("settings.wipe_button")}
          </Button>
        </Row>
      </Group>

      <Sheet open={confirm} title={t("settings.wipe_title")} onClose={() => !deleting && setConfirm(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("settings.wipe_heading")}</h2>
          <p className="m-0 text-[13px] text-subtle">
            {t("settings.wipe_body")}
          </p>
          <div className="flex justify-end gap-2">
            <Button disabled={deleting} onClick={() => setConfirm(false)}>
              {t("settings.cancel")}
            </Button>
            <Button
              variant="danger"
              disabled={deleting}
              onClick={async () => {
                setDeleting(true);
                try {
                  await store.deleteAllData();
                } catch (e) {
                  // The sheet stays open (and closable) so the user can try again or cancel.
                  setDeleting(false);
                  toast(describeError(e));
                  return;
                }
                setDeleting(false);
                setConfirm(false);
                toast(t("settings.wipe_done"));
              }}
            >
              {deleting ? t("settings.wipe_deleting") : t("settings.wipe_button")}
            </Button>
          </div>
        </div>
      </Sheet>
    </div>
  );
}
