import { t } from "../lib/i18n";

/** Errors from the Rust side arrive as `{ code, message }`. */
export class StoreError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(message);
    this.name = "StoreError";
  }
}

export function toStoreError(e: unknown): StoreError {
  if (e instanceof StoreError) return e;
  if (typeof e === "object" && e !== null && "code" in e && "message" in e) {
    const { code, message } = e as { code: unknown; message: unknown };
    return new StoreError(String(code), String(message));
  }
  return new StoreError("unknown", e instanceof Error ? e.message : String(e));
}

/** Why a meeting has no summary. Fixed reasons are shown in the app language by their code (meeting.json keeps
 *  Rust's English text); other failures show the stored message. */
export function summaryErrorText(err: { code: string; message: string } | undefined): string {
  if (!err) return t("meeting.noSummaryBody");
  switch (err.code) {
    case "waiting":
      return t("meeting.summaryTranscriptOnly");
    case "too_short":
      return t("meeting.summaryTooShort");
    case "no_engine":
      return t("meeting.summaryNoEngine");
    default:
      return err.message;
  }
}

/** Turns a failed store call into the one line the user sees. */
export function describeError(e: unknown): string {
  const err = toStoreError(e);
  switch (err.code) {
    case "not_yet_available":
      return t("kit.errorLater", { message: err.message });
    case "not_found":
      return t("kit.errorGone");
    case "refused":
      return err.message;
    case "unsupported_schema":
      return t("kit.errorNewer");
    default:
      return t("kit.errorGeneric");
  }
}
