import type { ReactNode } from "react";
import { Link } from "react-router";
import { useCharacter } from "../state/CharacterContext";
import { usePhone } from "../lib/phone";
import { EmptyState } from "./Card";

export function RequireCharacter({ children }: { children: ReactNode }) {
  const { snapshot, busy } = useCharacter();
  const phone = usePhone();
  if (snapshot) return <>{children}</>;
  return (
    <div className="page">
      <EmptyState title={busy ? "Opening character..." : "No character open"}>
        {!busy && <p>Pick a character from the <Link to="/characters">Characters</Link> screen, or create a new one.</p>}
        {/* B-48 O1: phone tabs are always tappable, so this screen offers the way back as a button. */}
        {!busy && phone && <Link className="btn btn-primary" to="/characters">Go to Start</Link>}
      </EmptyState>
    </div>
  );
}
