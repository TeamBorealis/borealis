import { useState } from "react";

/** A character portrait, or the initials placeholder when there is none or the image does not decode (a truncated import). */
export function Portrait({ src, name, className = "portrait" }: { src: string | null; name: string; className?: string }) {
  const [failed, setFailed] = useState<string | null>(null);
  if (src && failed !== src) return <img className={className} src={src} alt={`Portrait of ${name}`} onError={() => setFailed(src)} />;
  const initials = name.split(/\s+/).filter(Boolean).slice(0, 2).map((w) => w[0]!.toUpperCase()).join("");
  return <div className={className} role="img" aria-label={`Portrait placeholder for ${name}`}>{initials || "?"}</div>;
}
