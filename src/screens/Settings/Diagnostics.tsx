import { useState } from "react";
import { diagnosticsClient } from "../../data";
import { describeError } from "../../data/errors";
import { newIssueUrl, setupLines } from "../../lib/diagnostics";
import { useT } from "../../lib/i18n/I18nProvider";
import type { CheckResult } from "../../lib/setupCheck";
import { copyText } from "../../lib/share";
import { openLink } from "../../lib/windows";
import { Button, Sheet, useToast } from "../../ui";
import { Group, Row } from "./parts";

/** "Something not working?": Copy diagnostics (after a preview), Open log folder, Report a problem.
 *  Self-contained, so it can move with Check setup. `results` are the checks on screen, copied into the report. */
export function Diagnostics({ results }: { results?: CheckResult[] }) {
  const t = useT();
  const toast = useToast();
  const [loading, setLoading] = useState(false);
  const [report, setReport] = useState<string>();

  const prepare = async () => {
    setLoading(true);
    try {
      setReport(await diagnosticsClient.report(setupLines(results)));
    } catch (e) {
      toast(describeError(e));
    } finally {
      setLoading(false);
    }
  };

  const copy = async () => {
    if (report === undefined) return;
    try {
      await copyText(report);
      setReport(undefined);
      toast(t("settings.diag_copied"));
    } catch (e) {
      toast(describeError(e));
    }
  };

  return (
    <>
      <Group>
        <Row title={t("settings.diag_title")} desc={t("settings.diag_desc")} stack>
          <div className="flex flex-wrap items-center gap-2">
            <Button disabled={loading} onClick={() => void prepare()}>
              {loading ? t("settings.diag_loading") : t("settings.diag_copy")}
            </Button>
            <Button onClick={() => diagnosticsClient.revealLog().catch((e) => toast(describeError(e)))}>{t("settings.diag_open_log")}</Button>
            <Button variant="ghost" className="underline underline-offset-2" onClick={() => void openLink(newIssueUrl(__APP_VERSION__))}>
              {t("settings.diag_report")}
            </Button>
          </div>
        </Row>
      </Group>
      <Sheet open={report !== undefined} title={t("settings.diag_preview_title")} onClose={() => setReport(undefined)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("settings.diag_preview_title")}</h2>
          <p className="m-0 text-[13px] text-subtle">{t("settings.diag_preview_body")}</p>
          <pre
            aria-label={t("settings.diag_preview_title")}
            tabIndex={0}
            className="m-0 max-h-[46vh] overflow-auto whitespace-pre-wrap break-words rounded-control border border-line bg-rail p-3 font-mono text-[11px] leading-[16px]"
          >
            {report}
          </pre>
          <div className="flex justify-end gap-2">
            <Button onClick={() => setReport(undefined)}>{t("settings.cancel")}</Button>
            <Button variant="primary" onClick={() => void copy()}>
              {t("settings.diag_preview_copy")}
            </Button>
          </div>
        </div>
      </Sheet>
    </>
  );
}
