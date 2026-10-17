import type { FollowUpDraft } from "../data/store";

/**
 * Mail apps and the system that hands them a mailto: link cut long links off (Windows stops near 2,000 characters,
 * some mail apps sooner), so a longer email is offered for copying only.
 */
export const MAILTO_MAX = 1_800;

/** A mailto: link with the invitees, subject and body, or undefined when it would be too long to open safely. */
export function mailtoUrl(d: FollowUpDraft, max = MAILTO_MAX): string | undefined {
  // Each address keeps its literal @ (some mail apps don't read %40 in the address part); the parts around it are
  // escaped on their own.
  const to = d.to
    .map((a) => a.trim())
    .filter((a) => /^[^\s@,;?&]+@[^\s@,;?&]+$/.test(a))
    .map((a) => {
      const [local, domain] = a.split("@");
      return `${encodeURIComponent(local)}@${encodeURIComponent(domain)}`;
    });
  // Line breaks as CRLF, which RFC 6068 asks for and every mail app reads.
  const body = d.body.replace(/\r?\n/g, "\r\n");
  const url = `mailto:${to.join(",")}?subject=${encodeURIComponent(d.subject)}&body=${encodeURIComponent(body)}`;
  return url.length <= max ? url : undefined;
}

/** The email as one block of text for the clipboard: subject line, blank line, body. */
export const followUpText = (d: FollowUpDraft) => (d.subject.trim() ? `${d.subject.trim()}\n\n${d.body}` : d.body);
