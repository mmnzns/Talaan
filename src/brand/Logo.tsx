export function Logo({ size = 24 }: { size?: number }) {
  return (
    <svg role="img" aria-label="Talaan" width={size} height={size} viewBox="0 0 30 30">
      <rect width="30" height="30" rx="7" fill="#141416" />
      <rect x="7" y="8" width="16" height="2.6" rx="1.3" fill="#B5E61D" />
      <rect x="7" y="13.7" width="11" height="2.6" rx="1.3" fill="#F3F2EE" />
      <rect x="7" y="19.4" width="13" height="2.6" rx="1.3" fill="#F3F2EE" />
    </svg>
  );
}
