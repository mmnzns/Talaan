import { useState } from "react";
import type { Summary } from "../../data/types";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Sheet } from "../../ui";
import { summaryHasChanges } from "../../lib/summary";

/**
 * Writing the summary again replaces the old one, with the user's edits and ticked action items. When there are
 * any, `ask(run)` asks first; otherwise it runs straight away. Render `sheet` once.
 */
export function useConfirmRewrite(summary: Summary | undefined) {
  const t = useT();
  const [pending, setPending] = useState<() => void>();
  const ask = (run: () => void) => (summaryHasChanges(summary) ? setPending(() => run) : run());
  const close = () => setPending(undefined);
  const sheet = (
    <Sheet open={pending !== undefined} title={t("meeting.rewriteTitle")} onClose={close}>
      <div className="flex flex-col gap-3 p-5">
        <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("meeting.rewriteTitle")}</h2>
        <p className="m-0 text-[13px] text-subtle">{t("meeting.rewriteBody")}</p>
        <div className="flex justify-end gap-2">
          <Button onClick={close}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            onClick={() => {
              const run = pending;
              close();
              run?.();
            }}
          >
            {t("meeting.rewriteConfirm")}
          </Button>
        </div>
      </div>
    </Sheet>
  );
  return { ask, sheet };
}
