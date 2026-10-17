import type { CheckResult } from "./setupCheck";

/** Where "Report a problem" goes: a new GitHub issue, with the version filled in and a reminder to paste the
 *  diagnostics (nothing else is put in the address). */
export function newIssueUrl(version: string): string {
  const body = `Talaan version: ${version}\n\nWhat happened:\n\n\nWhat you expected:\n\n\nDiagnostics (Settings > General > Check setup > Copy diagnostics), pasted here:\n\n`;
  return `https://github.com/mmnzns/Talaan/issues/new?body=${encodeURIComponent(body)}`;
}

/** Check setup results as report lines: "fail · Speech model · No speech model yet, …". */
export function setupLines(results: CheckResult[] | undefined): string[] {
  return (results ?? []).map((r) => `${r.status} · ${r.title} · ${r.reportDetail ?? r.detail}`);
}
