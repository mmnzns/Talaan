import { useSettings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import { describeError } from "../../data/errors";
import type { Meeting } from "../../data/types";
import { useT } from "../../lib/i18n/I18nProvider";
import { currentSummaryLanguage } from "../../lib/summaryLanguages";
import { Button, Sheet, useToast } from "../../ui";
import { LanguagePicker } from "../LanguagePicker";
import { useConfirmRewrite } from "./ConfirmRewrite";

/**
 * ⋯ › Summarize in… and the summary card's Change language: pick a language and the summary is written again from the
 * transcript in it (not translated). A summary the user edited, or ticked, asks first, as Summarize as… does.
 */
export function SummarizeInSheet({ meeting, open, onClose }: { meeting: Meeting; open: boolean; onClose: () => void }) {
  const t = useT();
  const store = useStore();
  const toast = useToast();
  const { settings } = useSettings();
  const rewrite = useConfirmRewrite(meeting.summary);
  const current = currentSummaryLanguage(meeting, settings?.summaryLanguage);

  const pick = (code: string) => {
    onClose();
    rewrite.ask(() => {
      store.summarizeIn(meeting.id, code).catch((e) => toast(describeError(e)));
    });
  };

  return (
    <>
      <Sheet open={open} title={t("meeting.summarizeInTitle")} onClose={onClose}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("meeting.summarizeInTitle")}</h2>
          <p className="m-0 text-[13px] text-subtle">{t("meeting.summarizeInDesc")}</p>
          <LanguagePicker value={current} onPick={pick} localModel={settings?.summaryEngine === "local" ? settings.localModel : undefined} />
          <div className="flex justify-end gap-2">
            <Button onClick={onClose}>{t("common.cancel")}</Button>
          </div>
        </div>
      </Sheet>
      {rewrite.sheet}
    </>
  );
}
