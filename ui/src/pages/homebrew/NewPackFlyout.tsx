import { useState } from "react";
import type { BookRuleset, ContentPack } from "../../api/contract";
import { Alert, Button, ComboBox, Expander, Flyout, GuideLink, TextBox } from "../../components";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";
import { enterCreates } from "./flyoutKeys";
import "../characters/characters.css";

const EDITIONS: { value: BookRuleset; label: string }[] = [
  { value: "dnd2024", label: "2024" },
  { value: "dnd2014", label: "2014" },
  { value: "any", label: "Both editions" },
];

const LICENSES = [
  { value: "none", label: "All rights reserved" },
  { value: "CC-BY-4.0", label: "CC-BY-4.0" },
  { value: "CC0", label: "CC0" },
];

/** A pack code (spec 5.3): 2-8 capital letters or digits. */
export const CODE_PATTERN = /^[A-Z0-9]{2,8}$/;

/** The code suggested from a pack name: the words' initials ("Ember Isles Expanded" -> "EIE"), or a one-word name's first letters. */
export function suggestCode(name: string): string {
  const words = name.toUpperCase().normalize("NFKD").replace(/[^A-Z0-9\s]/g, " ").split(/\s+/).filter(Boolean);
  if (words.length === 0) return "";
  const code = words.length === 1 ? words[0]!.slice(0, 4) : words.map((w) => w[0]).join("").slice(0, 8);
  return code.length >= 2 ? code : words[0]!.slice(0, 2);
}

/**
 * NEW PACK (spec 14.1), in the New Character flyout's look (13.11): name, code (ids start with it and never change), edition,
 * author, description, licence; CREATE / CANCEL.
 */
export function NewPackFlyout({ onCreated, onClose }: { onCreated: (pack: ContentPack) => void; onClose: () => void }) {
  const api = useApi();
  const [name, setName] = useState("");
  const [code, setCode] = useState<string | null>(null);
  const [edition, setEdition] = useState<BookRuleset>("dnd2024");
  const [author, setAuthor] = useState("");
  const [description, setDescription] = useState("");
  const [license, setLicense] = useState("none");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const shownCode = code ?? suggestCode(name);
  const codeOk = CODE_PATTERN.test(shownCode);
  const create = async () => {
    if (!name.trim()) { setError("Give the pack a name."); return; }
    if (!codeOk) { setError("The pack code is 2 to 8 capital letters or digits."); return; }
    setBusy(true);
    try {
      const pack = await api.homebrewCreatePack({ request: { name: name.trim(), code: shownCode, edition, author: author.trim() || undefined, description: description.trim() || undefined, license } });
      onCreated(pack);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Flyout title="New pack" onClose={onClose} headAction={<GuideLink guide="homebrew-creator" />}
      footer={<div className="v4-foot"><Button data-testid="create-pack" disabled={busy} onClick={() => void create()}>Create</Button><Button onClick={onClose}>Cancel</Button></div>}>
      <form className="v4-new" onSubmit={(e) => { e.preventDefault(); void create(); }} onKeyDown={enterCreates(() => { if (!busy) void create(); })}>
        <Expander flyout title="Pack details">
          <p className="sub">Your own elements live in a pack. It loads like any other pack and travels with your other devices.</p>
          <label className="v4-fl" htmlFor="hb-pack-name">Name</label>
          <TextBox light id="hb-pack-name" value={name} placeholder="My homebrew" data-autofocus onChange={(e) => setName(e.target.value)} />
          <div className="v4-row">
            <div>
              <label className="v4-fl" htmlFor="hb-pack-code">Code</label>
              <TextBox light id="hb-pack-code" value={shownCode} maxLength={8} aria-describedby="hb-pack-code-hint" invalid={shownCode !== "" && !codeOk}
                onChange={(e) => setCode(e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ""))} />
            </div>
            <div>
              <label className="v4-fl" htmlFor="hb-pack-edition">Edition</label>
              <ComboBox light id="hb-pack-edition" options={EDITIONS} value={edition} onChange={(v) => setEdition(v as BookRuleset)} />
            </div>
          </div>
          <p className="sub hint" id="hb-pack-code-hint">2 to 8 capital letters or digits. It names the pack's elements inside Borealis and can't be changed later.</p>
          <label className="v4-fl" htmlFor="hb-pack-author">Author</label>
          <TextBox light id="hb-pack-author" value={author} onChange={(e) => setAuthor(e.target.value)} />
          <label className="v4-fl" htmlFor="hb-pack-desc">Description</label>
          <textarea id="hb-pack-desc" className="hb-flyout-text" value={description} onChange={(e) => setDescription(e.target.value)} />
          <label className="v4-fl" htmlFor="hb-pack-license">Licence</label>
          <ComboBox light id="hb-pack-license" options={LICENSES} value={license} onChange={setLicense} />
          {error && <Alert tone="danger" role="alert">{error}</Alert>}
        </Expander>
      </form>
    </Flyout>
  );
}
