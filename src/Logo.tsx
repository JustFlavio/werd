/** Werd mark: an amber tile with a "w" drawn as a single continuous stroke. */
export function Logo({ size = 28 }: { size?: number }) {
  return <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden className="logo">
    <rect width="32" height="32" rx="8" fill="var(--accent)" />
    <path d="M7.5 11 L11.5 22 L16 14 L20.5 22 L24.5 11" fill="none" stroke="var(--accent-ink)" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" />
  </svg>;
}
