import { useEffect, useId, useState } from "react";
import { Flyout } from "../controls/Flyout";
import { CheckBox, TextBox } from "../controls/Controls";
import { Button } from "..";
import { Portrait } from "../Portrait";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { isTauri } from "../../lib/tauri";
import { buildLine } from "./QuickViewFlyout";
import { useShell } from "./ShellContext";
import "./SaveFlyout.css";
import { setStatus } from "../../state/status";

const DEFAULT_GROUP = "Characters";
const ILLEGAL = /[\\/:*?"<>|\u0000-\u001f]/g;
/** Classic builder strips illegal filename characters silently as you type. */
export const sanitizeFileName = (s: string): string => s.replace(ILLEGAL, "");

/** Classic builder's Save character flyout (Ctrl+S, SAVE, File > Save): portrait, name and build, filename, Favorite, Group, then Save character / Cancel. */
export function SaveFlyout() {
  const { snapshot, saveOpen } = useCharacter();
  if (!saveOpen || !snapshot) return null;
  return <SaveForm key={snapshot.id} />;
}

function SaveForm() {
  const api = useApi();
  const { snapshot, closeSave, commitSave } = useCharacter();
  const { setPortraitsOpen } = useShell();
  const s = snapshot!;
  /** The filename is its own field: editing it never renames the character (rename with Edit on the tile). */
  const [fileName, setFileName] = useState(s.name);
  const [favorite, setFavorite] = useState(s.favorite);
  const [group, setGroup] = useState(s.group || DEFAULT_GROUP);
  const [groups, setGroups] = useState<string[]>([DEFAULT_GROUP]);
  const [working, setWorking] = useState(false);
  const listId = useId();

  useEffect(() => {
    let live = true;
    api.listCharacters().then((l) => live && setGroups([...new Set([DEFAULT_GROUP, ...l.map((c) => c.group).filter(Boolean)])])).catch(() => undefined);
    return () => { live = false; };
  }, [api]);

  const file = sanitizeFileName(fileName).trim();
  const save = async () => {
    setWorking(true);
    const ok = await commitSave({ name: s.name, group: group.trim() || DEFAULT_GROUP, favorite });
    setWorking(false);
    if (ok) closeSave();
  };
  /** Save As: the backend opens its native dialog and returns the snapshot; the webview never sees a raw path. */
  const saveAs = async () => {
    setWorking(true);
    try {
      const next = await api.saveCharacterAs({ characterId: s.id });
      setStatus(`'${next.name}' Saved`);
      closeSave();
    } catch {
      setStatus("Canceled Saved");
    } finally {
      setWorking(false);
    }
  };

  return (
    <Flyout title="Save character" onClose={closeSave} footer={
      <div className="v4-foot">
        <Button variant="highlight" onClick={() => void save()} disabled={working}>Save character</Button>
        {isTauri() && <Button onClick={() => void saveAs()} disabled={working}>Save as…</Button>}
        <Button onClick={closeSave}>Cancel</Button>
      </div>
    }>
      <div className="sv">
        <button type="button" className="sv-portrait" aria-label="Change portrait" onClick={() => { closeSave(); setPortraitsOpen(true); }}>
          <Portrait src={s.portrait} name={s.name} className="qv-portrait" />
        </button>
        <h3 className="qv-build">{s.name}</h3>
        <p className="qv-sub">{buildLine(s)}</p>
        <div className="field">
          <label htmlFor={`${listId}-n`}>Filename</label>
          <TextBox light id={`${listId}-n`} value={fileName} onChange={(e) => setFileName(sanitizeFileName(e.target.value))} onKeyDown={(e) => e.key === "Enter" && void save()} />
          <p className="field-hint">{file ? `${file}.json` : ""}</p>
        </div>
        <label className="sv-fav"><CheckBox checked={favorite} onChange={(e) => setFavorite(e.target.checked)} /> Favorite</label>
        <div className="field">
          <label htmlFor={`${listId}-g`}>Group</label>
          <TextBox light id={`${listId}-g`} list={listId} value={group} onChange={(e) => setGroup(e.target.value)} />
          <datalist id={listId}>{groups.map((g) => <option key={g} value={g} />)}</datalist>
        </div>
      </div>
    </Flyout>
  );
}
