import type { ReactNode } from "react";
import { Link } from "react-router";
import { useCharacter } from "../state/CharacterContext";
import { EmptyState } from "./Card";

export function RequireCharacter({ children }: { children: ReactNode }) {
  const { snapshot, busy } = useCharacter();
  if (snapshot) return <>{children}</>;
  return (
    <div className="page">
      <EmptyState title={busy ? "Opening character..." : "No character open"}>
        {!busy && <p>Pick a character from the <Link to="/characters">Characters</Link> screen, or create a new one.</p>}
      </EmptyState>
    </div>
  );
}
