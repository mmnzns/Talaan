import { describeError } from "../../data/errors";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, cn } from "../../ui";

/**
 * "Couldn't save your settings": the change is still shown but wasn't written. Shared by every screen that changes
 * a setting (from `useSettings().saveError`), so the wording and the two ways out (try again, undo) are the same.
 */
export function SaveFailedNotice({ cause, onRetry, onUndo, className }: { cause: unknown; onRetry: () => void; onUndo: () => void; className?: string }) {
  const t = useT();
  return (
    <div role="alert" className={cn("flex flex-col gap-2 rounded-card border border-line bg-warn-bg p-4", className)}>
      <span className="text-[13px] font-bold">{t("settings.save_failed_title")}</span>
      <span className="text-xs leading-[17px] text-subtle">{t("settings.save_failed_body", { detail: describeError(cause) })}</span>
      <div className="flex gap-2">
        <Button variant="primary" onClick={onRetry}>
          {t("settings.save_retry")}
        </Button>
        <Button onClick={onUndo}>{t("settings.save_undo")}</Button>
      </div>
    </div>
  );
}
