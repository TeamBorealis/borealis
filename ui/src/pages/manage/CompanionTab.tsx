import { mdiImageSearch } from "@mdi/js";
import { useState } from "react";
import { Button, TextBox } from "../../components";
import { useCharacter } from "../../state/CharacterContext";
import { MgField, MgSection } from "./Field";
import { PortraitsFlyout } from "./PortraitsFlyout";

const SUB_HEADER = "A companion that accompanies you on your adventures and is trained to fight alongside you.";

/**
 * The classic builder's COMPANION tab: header, one sentence, the Companion Name field (the creature's name until the user types another;
 * clearing it restores that name) and the Companions Gallery button. The statistics live in the information slider's companion context.
 */
export function CompanionTab() {
  const { snapshot, mutate } = useCharacter();
  const [galleryOpen, setGalleryOpen] = useState(false);
  const c = snapshot?.companion;
  if (!c) return null;
  const original = c.creature?.name ?? "";
  const rename = (typed: string) => {
    const name = typed.trim() === original ? "" : typed.trim();
    void mutate((api, cid) => api.updateCompanion({ characterId: cid, patch: { name } }));
  };
  return (
    <div className="mg-page">
      <MgSection title="COMPANION" sub={SUB_HEADER}>
        <div className="mg-grid">
          <MgField label="Companion Name">
            {(id) => <TextBox key={c.name} id={id} defaultValue={c.name} onBlur={(e) => rename(e.target.value)} />}
          </MgField>
        </div>
        <div style={{ marginTop: 10 }}>
          <Button variant="flat" onClick={() => setGalleryOpen(true)}>
            <svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={mdiImageSearch} /></svg>Companions Gallery
          </Button>
        </div>
      </MgSection>
      {galleryOpen && (
        <PortraitsFlyout current={c.portrait} onClose={() => setGalleryOpen(false)}
          onSelect={(portrait) => void mutate((api, cid) => api.updateCompanion({ characterId: cid, patch: { portrait } }))} />
      )}
    </div>
  );
}
