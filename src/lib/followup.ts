import type { FollowUpDraft } from "../data/store";
import { MAILTO_MAX, mailtoLink } from "./mailto";

/** A longer email than this is offered for copying only (see `mailto.ts`). */
export { MAILTO_MAX };

/** A mailto: link with the invitees, subject and body, or undefined when it would be too long to open safely. */
export function mailtoUrl(d: FollowUpDraft, max = MAILTO_MAX): string | undefined {
  const url = mailtoLink(d.to, d.subject, d.body);
  return url.length <= max ? url : undefined;
}

/** The email as one block of text for the clipboard: subject line, blank line, body. */
export const followUpText = (d: FollowUpDraft) => (d.subject.trim() ? `${d.subject.trim()}\n\n${d.body}` : d.body);
