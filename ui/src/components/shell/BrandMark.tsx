/** Original Borealis mark (30 x 24): twin peaks under an aurora ribbon. Not derived from any third-party artwork. */
export function BrandMark({ className = "" }: { className?: string }) {
  return (
    <svg className={`brand-mark ${className}`} viewBox="0 0 30 24" width="30" height="24" aria-hidden="true" focusable="false">
      <path d="M0 22 L9 8 L14 15 L19 5 L30 22 Z" fill="#305754" />
      <path d="M2 11 C8 2 12 12 17 6 S25 1 29 6" fill="none" stroke="#6fd0b5" strokeWidth="2" strokeLinecap="round" />
      <path d="M5 22 L10 14 L14 20 L19 11 L25 22 Z" fill="#1b2d2b" />
    </svg>
  );
}
