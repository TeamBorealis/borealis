export function Portrait({ src, name, className = "portrait" }: { src: string | null; name: string; className?: string }) {
  if (src) return <img className={className} src={src} alt={`Portrait of ${name}`} />;
  const initials = name.split(/\s+/).filter(Boolean).slice(0, 2).map((w) => w[0]!.toUpperCase()).join("");
  return <div className={className} role="img" aria-label={`Portrait placeholder for ${name}`}>{initials || "?"}</div>;
}
