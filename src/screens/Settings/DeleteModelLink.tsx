import { useState } from "react";
import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import { formatBytes } from "../../lib/format";
import { useLang, useT } from "../../lib/i18n/I18nProvider";
import { deletedMessage } from "../../lib/modelDelete";
import { Button, Sheet, useToast } from "../../ui";

interface Props {
  id: string;
  name: string;
  sizeMb: number;
}

/**
 * The small "Delete" link on a downloaded model. A model is a multi-GB download, so the link opens a sheet that
 * names the model and says what Delete means: Talaan stops using it, and the engine erases the file only when no
 * other app uses it. Cancel has focus. After the delete, the same toast as before says when the file stays.
 */
export function DeleteModelLink({ id, name, sizeMb }: Props) {
  const store = useSettingsStore();
  const toast = useToast();
  const t = useT();
  const lang = useLang();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);

  const confirm = async () => {
    setBusy(true);
    try {
      const r = await store.deleteModel(id);
      const msg = deletedMessage(t, lang, name, r);
      if (msg) toast(msg);
    } catch (e) {
      toast(describeError(e));
    }
    setBusy(false);
    setOpen(false);
  };

  return (
    <>
      <button
        type="button"
        className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline"
        aria-label={t("settings.model_delete_link", { name })}
        onClick={() => setOpen(true)}
      >
        {t("settings.delete")}
      </button>
      <Sheet open={open} title={t("settings.model_delete_title")} onClose={() => !busy && setOpen(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("settings.model_delete_heading", { name })}</h2>
          <p className="m-0 text-[13px] text-subtle">{t("settings.model_delete_body", { size: formatBytes(sizeMb * 1_000_000) })}</p>
          <div className="flex justify-end gap-2">
            <Button data-autofocus disabled={busy} onClick={() => setOpen(false)}>
              {t("settings.cancel")}
            </Button>
            <Button variant="danger" disabled={busy} onClick={confirm}>
              {t("settings.delete")}
            </Button>
          </div>
        </div>
      </Sheet>
    </>
  );
}
