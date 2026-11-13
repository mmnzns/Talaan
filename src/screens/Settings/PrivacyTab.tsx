import { useEffect, useRef, useState } from "react";
import { useMeetings } from "../../data/hooks";
import { describeError } from "../../data/errors";
import { useSettingsStore, useStore } from "../../data/StoreProvider";
import { exportAll } from "../../lib/exportAll";
import type { RetentionChoice, Settings, TranscriptRetention } from "../../data/settings";
import { formatBytes, tildePath } from "../../lib/format";
import type { MessageKey } from "../../lib/i18n";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Select, Sheet, useToast } from "../../ui";
import { LeavesTable } from "./LeavesTable";
import { Group, Row } from "./parts";
import type { TabProps } from "./tabProps";

/** A retention choice that would delete something now, waiting for a yes. */
type Pending = { patch: Partial<Settings>; kind: "audio" | "meetings"; count: number; period: MessageKey | null };

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

  const [pending, setPending] = useState<Pending>();
  const asked = useRef(0);

  // A new limit saves at once unless it would delete something right now (the Rust rules count it): then it waits
  // for a yes. Picking an option moves the setting only after the save, so the select shows the old value meanwhile.
  const choose = async (patch: Partial<Settings>, kind: Pending["kind"]) => {
    const mine = ++asked.current;
    const audio = patch.retention ?? s.retention;
    const transcripts = patch.transcriptRetention ?? s.transcriptRetention;
    try {
      const n = await store.retentionPreview(audio, transcripts);
      if (mine !== asked.current) return; // a newer pick (arrowing through the options) took over
      const count = kind === "audio" ? n.audio : n.meetings;
      if (count === 0) return void update(patch);
      const periods: Record<string, MessageKey> = { "1d": "settings.keep_1d", "7d": "settings.keep_7d", "30d": "settings.keep_30d", "1y": "settings.keep_1y", "90d": "settings.keep_90d" };
      const period = periods[kind === "audio" ? audio : transcripts];
      setPending({ patch, kind, count, period: period ?? null });
    } catch (e) {
      // Without the count there's no honest confirmation, so nothing is saved.
      if (mine === asked.current) toast(describeError(e));
    }
  };
  const confirmPending = async () => {
    if (!pending) return;
    setPending(undefined);
    await update(pending.patch).catch((e) => toast(describeError(e)));
  };

  useEffect(() => {
    store.dataInfo().then(setInfo, () => setInfo(undefined));
  }, [store, meetings.length]);

  return (
    <div className="flex flex-col gap-3">
      <Group>
        <LeavesTable settings={s} />
        <Row title={t("settings.audio_title")} desc={t("settings.audio_desc")}>
          <Select label={t("settings.audio_title")} wrapperClassName="w-40" value={s.retention} onChange={(e) => void choose({ retention: e.target.value as RetentionChoice }, "audio")}>
            <option value="1d">{t("settings.keep_1d")}</option>
            <option value="7d">{t("settings.keep_7d")}</option>
            <option value="30d">{t("settings.keep_30d")}</option>
            <option value="forever">{t("settings.keep_forever")}</option>
            <option value="none">{t("settings.keep_none")}</option>
          </Select>
        </Row>
        <Row title={t("settings.transcripts_title")} desc={t("settings.transcripts_desc")}>
          <Select label={t("settings.transcripts_aria")} wrapperClassName="w-40" value={s.transcriptRetention} onChange={(e) => void choose({ transcriptRetention: e.target.value as TranscriptRetention }, "meetings")}>
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

      <Sheet open={!!pending} title={t("settings.retention_title")} onClose={() => setPending(undefined)}>
        {pending && (
          <div className="flex flex-col gap-3 p-5">
            <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t(`settings.retention_${pending.kind}_heading`, { count: pending.count })}</h2>
            <p className="m-0 text-[13px] text-subtle">
              {pending.period
                ? t(`settings.retention_${pending.kind}_body`, { period: t(pending.period) })
                : t("settings.retention_audio_body_none")}
            </p>
            <div className="flex justify-end gap-2">
              <Button data-autofocus onClick={() => setPending(undefined)}>
                {t("settings.cancel")}
              </Button>
              <Button variant="danger" onClick={confirmPending}>
                {pending.kind === "audio" ? t("settings.retention_audio_button") : t("settings.retention_meetings_button", { count: pending.count })}
              </Button>
            </div>
          </div>
        )}
      </Sheet>

      <Sheet open={confirm} title={t("settings.wipe_title")} onClose={() => !deleting && setConfirm(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("settings.wipe_heading")}</h2>
          <p className="m-0 text-[13px] text-subtle">
            {t("settings.wipe_body")}
            {s.autoSaveFolder ? ` ${t("settings.wipe_keeps_copies")}` : ""}
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
