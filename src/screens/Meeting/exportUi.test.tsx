import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { StoreProvider } from "../../data/StoreProvider";
import { MockMeetingStore } from "../../data/mock/store";
import { MockSettingsStore } from "../../data/mock/settings";
import { seedMeetings } from "../../data/mock/seed";
import { ExportSheet } from "./ExportSheet";
import { MeetingHeader } from "./MeetingHeader";

const q4 = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-q4")!;
const settings = new MockSettingsStore();
const meetings = new MockMeetingStore(new Date("2026-10-14T20:00:00"), { yourName: () => "Me" });
const wrap = (node: React.ReactNode) => renderToStaticMarkup(<StoreProvider meetings={meetings} settings={settings}>{node}</StoreProvider>);

describe("meeting header", () => {
  const html = wrap(<MeetingHeader meeting={q4} editing={false} canRevert={false} onToggleEdit={() => {}} onRevert={() => {}} onExport={() => {}} onEmail={() => {}} onCopyEmail={() => {}} />);
  it("has Export and no Copy as text button", () => {
    expect(html).toContain(">Export<");
    expect(html).not.toContain("Copy as text");
    expect(html).not.toContain(">Copied<");
  });
});

describe("export sheet", () => {
  const sheet = (props: Partial<React.ComponentProps<typeof ExportSheet>> = {}) => wrap(<ExportSheet open meeting={q4} onClose={() => {}} {...props} />);
  const checked = (html: string, label: string) => new RegExp(`role="radio" aria-checked="true"[^>]*>(?:(?!</button>).)*?${label}<`).test(html);

  it("offers Copy as text as a format", () => {
    const html = sheet();
    expect(html).toContain("Copy as text");
    expect(checked(html, "Copy as text")).toBe(true);
  });
  it("asks what to include, with Both chosen by default", () => {
    const html = sheet();
    expect(html).toContain('aria-label="Include"');
    for (const label of ["Summary", "Transcript", "Both"]) expect(html).toContain(`>${label}<`);
    expect(checked(html, "Both")).toBe(true);
    expect(checked(html, "Summary")).toBe(false);
    expect(html).not.toContain("Include transcript");
  });
  it("shows the choice for every format but email, which is always the summary", () => {
    const hidden = (html: string) => /class="[^"]*hidden[^"]*"[^>]*><div role="radiogroup" aria-label="Include"/.test(html);
    expect(hidden(sheet())).toBe(false);
    expect(hidden(sheet({ initialFormat: "md" }))).toBe(false);
    expect(hidden(sheet({ initialFormat: "email" }))).toBe(true);
  });
});
