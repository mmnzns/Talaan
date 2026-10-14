export function MeetingPane({ id }: { id?: string; jumpTo?: number }) {
  return <div className="p-6">{id ?? "Select a meeting"}</div>;
}
