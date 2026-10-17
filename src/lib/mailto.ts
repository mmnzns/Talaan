/**
 * The one mailto: builder, for Email this summary (`share.ts`) and Draft follow-up (`followup.ts`). Recipients come
 * from the calendar, which has already left out people who declined, rooms and equipment, and the user (Rust's
 * calendar reading, on both platforms).
 *
 * Mail apps and the system that hands them a mailto: link cut long links off (Windows stops near 2,000 characters,
 * some mail apps sooner), so links are kept within `MAILTO_MAX`.
 */
export const MAILTO_MAX = 1_800;

/** Valid addresses only, each once, each keeping its literal @ (some mail apps don't read %40 in the address part);
 *  the parts around it are escaped on their own. */
export function mailtoRecipients(addresses: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of addresses) {
    const a = raw.trim();
    if (!/^[^\s@,;?&]+@[^\s@,;?&]+$/.test(a) || seen.has(a.toLowerCase())) continue;
    seen.add(a.toLowerCase());
    const [local, domain] = a.split("@");
    out.push(`${encodeURIComponent(local)}@${encodeURIComponent(domain)}`);
  }
  return out;
}

/** A subject or body for the link, line breaks as CRLF (RFC 6068 asks for it and every mail app reads it). */
export const mailtoPart = (s: string) => encodeURIComponent(s.replace(/\r?\n/g, "\r\n"));

/** Everything up to the body: `mailto:to?subject=…&body=`. */
export const mailtoHead = (to: string[], subject: string) => `mailto:${mailtoRecipients(to).join(",")}?subject=${mailtoPart(subject)}&body=`;

export const mailtoLink = (to: string[], subject: string, body: string) => mailtoHead(to, subject) + mailtoPart(body);
