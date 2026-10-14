export function MeetingList({ selectedId }: { selectedId?: string }) {
  return <nav aria-label="Meetings" className="w-[320px] border-r-[1.5px] border-edge bg-rail" data-selected={selectedId} />;
}
