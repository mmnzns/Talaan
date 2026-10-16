import { useState } from "react";
import { diagnosticsClient } from "../../data";
import { describeError } from "../../data/errors";
import { newIssueUrl, setupLines } from "../../lib/diagnostics";
import { useT } from "../../lib/i18n/I18nProvider";
import type { CheckResult } from "../../lib/setupCheck";
import { copyText } from "../../lib/share";
import { openLink } from "../../lib/windows";
import { Button, useToast } from "../../ui";
import { Group, Row } from "./parts";

/** "Something not working?": Copy diagnostics, Open log folder, Report a problem. Self-contained, so it can move
 *  with Check setup. `results` are the checks on screen, copied into the report. */
export function Diagnostics({ results }: { results?: CheckResult[] }) {
  const t = useT();
  const toast = useToast();
  const [copying, setCopying] = useState(false);

  const copy = async () => {
    setCopying(true);
    try {
      await copyText(await diagnosticsClient.report(setupLines(results)));
      toast(t("settings.diag_copied"));
    } catch (e) {
      toast(describeError(e));
    } finally {
      setCopying(false);
    }
  };

  return (
    <Group>
      <Row title={t("settings.diag_title")} desc={t("settings.diag_desc")} stack>
        <div className="flex flex-wrap items-center gap-2">
          <Button disabled={copying} onClick={() => void copy()}>
            {copying ? t("settings.diag_copying") : t("settings.diag_copy")}
          </Button>
          <Button onClick={() => diagnosticsClient.revealLog().catch((e) => toast(describeError(e)))}>{t("settings.diag_open_log")}</Button>
          <Button variant="ghost" className="underline underline-offset-2" onClick={() => void openLink(newIssueUrl(__APP_VERSION__))}>
            {t("settings.diag_report")}
          </Button>
        </div>
      </Row>
    </Group>
  );
}
