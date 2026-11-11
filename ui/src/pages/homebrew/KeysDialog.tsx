import { Button, Dialog } from "../../components";
import { CREATOR_KEYMAP, type KeyScope } from "./keymap";

const SCOPE_WORDS: Readonly<Record<KeyScope, string>> = { creator: "Anywhere in the creator", block: "On the focused rule or row" };

/** Ctrl reads Cmd on macOS (the shell's own rule). */
const onMac = (): boolean => typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
export const keysText = (keys: string, mac = onMac()): string => (mac ? keys.replace(/Ctrl\+/g, "Cmd+") : keys);

/**
 * The creator's keyboard map (spec 12.2), every binding with what it does, in the spec's order. Opened from the workspace's
 * "Keys" button; every action also has its own button or menu item (their tooltips name the key).
 */
export function KeysDialog({ onClose }: { onClose: () => void }) {
  const scopes: KeyScope[] = ["creator", "block"];
  return (
    <Dialog title="Keyboard shortcuts" onClose={onClose} className="hb-keys-dialog" footer={<Button variant="primary" onClick={onClose}>Close</Button>}>
      {scopes.map((scope) => (
        <section key={scope}>
          <h3 className="hb-section-title">{SCOPE_WORDS[scope]}</h3>
          <table className="hb-keys">
            <tbody>
              {CREATOR_KEYMAP.filter((b) => b.scope === scope).map((b) => (
                <tr key={b.action + b.keys}><th scope="row"><kbd>{keysText(b.keys)}</kbd></th><td>{b.label}</td></tr>
              ))}
              {scope === "creator" && <tr><th scope="row"><kbd>Enter</kbd></th><td>On a picker field: open the picker; type to search, Enter chooses</td></tr>}
            </tbody>
          </table>
        </section>
      ))}
    </Dialog>
  );
}
