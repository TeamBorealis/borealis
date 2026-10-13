import { Button } from "../../components";
import { DetailField } from "./DetailField";
import { MgSection } from "./Field";

const TRINKETS = ["A mummified goblin hand", "A crystal that glows faintly in moonlight", "A tiny silver bell without a clapper", "A key that fits no known lock", "A pressed flower from a forgotten garden"];

export function BackstoryTab({ onTrinket }: { onTrinket: (t: string) => void }) {
  return (
    <div className="mg-page">
      <MgSection title="Backstory" sub="Where did your character come from, and what shaped them?">
        <DetailField k="backstory" label="Backstory" area tall />
      </MgSection>
      <MgSection title="Personality" sub="Traits, ideals, bonds and flaws that drive the character at the table.">
        <div className="mg-grid">
          <DetailField k="personalityTraits" label="Personality traits" area className="span3" />
          <DetailField k="ideal" label="Ideal" area />
          <DetailField k="bond" label="Bond" area />
          <DetailField k="flaw" label="Flaw" area />
          <div className="mg-row span2">
            <div style={{ flex: 1, minWidth: 200 }}><DetailField k="trinket" label="Trinket" list={TRINKETS} /></div>
            <Button icon="dice" style={{ marginBottom: 4 }} onClick={() => onTrinket(TRINKETS[Math.floor(Math.random() * TRINKETS.length)]!)}>Random</Button>
          </div>
        </div>
      </MgSection>
    </div>
  );
}

export function NotesTab() {
  return (
    <div className="mg-page">
      <MgSection title="Notes" sub="Free-form notes printed on the character sheet, in two columns.">
        <div className="mg-two">
          <DetailField k="notesLeft" label="Left column" area tall />
          <DetailField k="notesRight" label="Right column" area tall />
        </div>
      </MgSection>
    </div>
  );
}

export function AlliesTab() {
  return (
    <div className="mg-page">
      <MgSection title="Allies and organizations" sub="Groups your character belongs to and the people who stand with them.">
        <div className="mg-grid two">
          <DetailField k="organization" label="Organization name" className="span2" />
          <DetailField k="allies" label="Allies" area className="span2" />
        </div>
      </MgSection>
    </div>
  );
}

export function TreasureTab() {
  return (
    <div className="mg-page">
      <MgSection title="Treasure" sub="Additional treasure and quest items your character carries, printed on the sheet.">
        <div className="mg-two">
          <DetailField k="treasure" label="Additional treasure" area tall />
          <DetailField k="questItems" label="Quest items" area tall />
        </div>
      </MgSection>
    </div>
  );
}
