import { FileAudio } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { describeError } from "../../data/errors";
import { call, onEvent } from "../../data/tauri/invoke";
import { ImportQueue, partitionImports, type BatchSummary, type ImportOutcome, type QueueSnapshot } from "../../lib/importQueue";
import { navigate, routes } from "../../lib/router";
import { inTauri } from "../../lib/windows";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Label, Sheet, useToast } from "../../ui";

interface Props {
  open: boolean;
  /** Files dropped on the main window: imported right away, one after another. A new array is a new drop. */
  dropped?: string[];
  onClose: () => void;
  /** Called when files failed, so the reasons are in front of the user even if the sheet was hidden. */
  onShow: () => void;
}

type ImportEvent = { state: "progress"; progress: number } | { state: "done"; id: string } | { state: "failed"; message: string };

export function ImportSheet({ open, dropped, onClose, onShow }: Props) {
  const t = useT();
  const toast = useToast();
  const [snap, setSnap] = useState<QueueSnapshot>();
  const [fileProgress, setFileProgress] = useState(0);
  const [notice, setNotice] = useState<string>();
  const waiter = useRef<(o: ImportOutcome) => void>(undefined);
  const extensions = useRef<Promise<string[] | null>>(undefined);

  // The queue outlives renders; it reads the latest callbacks through this ref.
  const latest = useRef({ t, toast, onClose, onShow });
  latest.current = { t, toast, onClose, onShow };

  const queue = useRef<ImportQueue>(undefined);
  if (!queue.current) {
    queue.current = new ImportQueue({
      importOne: (path) =>
        new Promise<ImportOutcome>((resolve) => {
          setFileProgress(0);
          waiter.current = resolve;
          call("meetings_import", { path }).catch((e) => resolve({ ok: false, message: describeError(e) }));
        }),
      onChange: setSnap,
      onFinish: (summary) => finish(summary, latest.current),
    });
  }

  useEffect(() => {
    if (!inTauri()) return;
    return onEvent<ImportEvent>("import-audio", (e) => {
      if (e.state === "progress") setFileProgress(e.progress);
      if (e.state === "done") waiter.current?.({ ok: true, id: e.id });
      if (e.state === "failed") waiter.current?.({ ok: false, message: e.message });
    });
  }, []);

  const addFiles = async (paths: string[]) => {
    setNotice(undefined);
    extensions.current ??= call<string[]>("import_extensions").catch(() => null);
    const allowed = await extensions.current;
    const { accepted, unsupported } = allowed ? partitionImports(paths, allowed) : { accepted: [...new Set(paths)], unsupported: [] };
    queue.current?.add(accepted, unsupported);
  };

  useEffect(() => {
    if (dropped?.length) void addFiles(dropped);
  }, [dropped]);

  const choose = async () => {
    if (!inTauri()) return setNotice(t("shell.importOnlyApp"));
    const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
    const allowed = await call<string[]>("import_extensions");
    const picked = await openDialog({ multiple: true, directory: false, filters: [{ name: t("shell.importAudioFilter"), extensions: allowed }] });
    if (Array.isArray(picked)) await addFiles(picked);
    else if (typeof picked === "string") await addFiles([picked]);
  };

  const working = !!snap?.running;
  const close = () => {
    if (!working) {
      setSnap(undefined);
      setNotice(undefined);
    }
    onClose();
  };

  const total = Math.max(snap?.total ?? 1, 1);
  const barPct = snap ? Math.round((Math.min(snap.finished + fileProgress, total) / total) * 100) : 0;
  const failures = !working && snap ? snap.failures : [];

  return (
    <Sheet open={open} title={t("shell.importTitle")} onClose={close}>
      <div className="flex flex-col gap-4 p-5">
        <div>
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("shell.importTitle")}</h2>
          <p className="m-0 mt-1 text-[13px] text-subtle">{t("shell.importIntro")}</p>
        </div>
        <div className="grid place-items-center gap-2 rounded-card border border-dashed border-input bg-surface px-4 py-10 text-center">
          <FileAudio size={28} aria-hidden="true" className="text-subtle" />
          {working ? (
            <div className="flex w-64 flex-col gap-2" role="status">
              <span className="truncate text-[13px] font-semibold">{t("shell.importReading", { name: snap?.current ?? "" })}</span>
              {total > 1 && <span className="font-mono text-[11px] text-subtle">{t("shell.importCount", { n: Math.min((snap?.finished ?? 0) + 1, total), total })}</span>}
              <span className="h-[5px] overflow-hidden rounded-full bg-line">
                <span className="block h-full bg-ink" style={{ width: `${barPct}%` }} />
              </span>
            </div>
          ) : (
            <>
              <span className="text-[13px] font-semibold">{t("shell.importDrag")}</span>
              <Label>.m4a · .mp3 · .wav · .aiff · .flac</Label>
              <Button data-autofocus onClick={() => void choose()}>
                {t("shell.importChoose")}
              </Button>
            </>
          )}
          {notice && (
            <span role="alert" className="text-xs font-semibold text-danger">
              {notice}
            </span>
          )}
          {failures.length > 0 && (
            <ul role="alert" className="m-0 flex max-h-32 w-full list-none flex-col gap-1 overflow-auto p-0 text-left text-xs text-danger">
              {failures.map((f, i) => (
                <li key={i} className="break-words">
                  <span className="font-semibold">{f.name}</span>: {f.unsupported ? t("shell.importUnsupported") : f.message}
                </li>
              ))}
            </ul>
          )}
        </div>
        <p className="m-0 text-xs text-subtle">{t("shell.importNote")}</p>
        <div className="flex justify-end">
          <Button onClick={close}>{working ? t("shell.hide") : t("common.cancel")}</Button>
        </div>
      </div>
    </Sheet>
  );
}

type Latest = { t: ReturnType<typeof useT>; toast: (m: string) => void; onClose: () => void; onShow: () => void };

/** What happens when the queue runs dry: one meeting opens it; several say how many; failures stay on screen. */
function finish({ total, imported, failures }: BatchSummary, { t, toast, onClose, onShow }: Latest) {
  if (failures.length > 0) {
    if (total > 1) toast(imported.length > 0 ? t("shell.importPartial", { ok: imported.length, total }) : t("shell.importNone", { total }));
    onShow();
    return;
  }
  onClose();
  if (imported.length === 1) navigate(routes.meeting(imported[0]));
  else toast(t("shell.importedMany", { count: imported.length }));
}
