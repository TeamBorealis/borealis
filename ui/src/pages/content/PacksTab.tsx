import { useEffect, useRef } from "react";
import type { ContentPack } from "../../api/contract";
import { Button, useConfirm, useImportNotice, useToast } from "../../components";
import { filesToInputs } from "../../lib/files";
import { startImportPreview } from "../../state/importPreview";
import { usePlatform } from "../../state/platform";
import { useApi } from "../../state/ApiContext";
import { errorMessage, useAsync } from "../../state/useAsync";
import { ImportLink } from "../import/ImportLink";
import { CheckBox } from "../../components/controls";
import { Section } from "./Section";

const CONTENT_ACCEPT = ".xml,.index,.zip,.json,application/json,text/xml,application/zip";
/** Android's chooser filters by MIME type and has none for `.index`; files without a known type are
 *  `application/octet-stream`, so phones also accept that (the backend still checks every file). */
const UNTYPED_FILE = "application/octet-stream";

export function PacksTab() {
  const api = useApi();
  const toast = useToast();
  const confirm = useConfirm();
  const notice = useImportNotice();
  const packs = useAsync(() => api.listPacks(), [api]);
  const { mobile } = usePlatform();
  const fileRef = useRef<HTMLInputElement>(null);
  useEffect(() => api.subscribe("content:reloaded", packs.reload), [api, packs.reload]);
  const preview = (request: Parameters<typeof api.previewImportPack>[0]["request"]) => startImportPreview(api, request, (m) => toast.push(m, "error"));
  /** True once the notice was accepted for the picker just opened, so choosing files does not ask a second time. */
  const acknowledged = useRef(false);
  const onFiles = async (list: FileList | null) => {
    const files = Array.from(list ?? []);
    if (fileRef.current) fileRef.current.value = "";
    const ok = acknowledged.current || (files.length > 0 && await notice.ensure());
    acknowledged.current = false;
    if (ok && files.length) await preview({ files: await filesToInputs(files) });
  };
  const pickFolder = async () => {
    if (!(await notice.ensure())) return;
    try {
      const [picked] = await api.pickPath({ purpose: "content-folder" });
      if (picked) await preview({ folder: picked.token });
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };
  const chooseFiles = async () => { if (!(await notice.ensure())) return; acknowledged.current = true; fileRef.current?.click(); };
  const remove = async (p: ContentPack) => {
    const r = await confirm({ title: "Remove pack", body: `Remove "${p.name}"? Characters using its content may lose selections.`, confirmLabel: "Remove", danger: true });
    if (r === "confirm") { await api.removePack({ packId: p.id }).catch((e) => toast.push(errorMessage(e), "error")); packs.reload(); }
  };
  return (
    <>
      <Section title="Custom content" actions={<>
        <ImportLink />
        {!mobile && <Button icon="upload" onClick={() => void pickFolder()}>Import my content folder</Button>}
        <Button icon="upload" onClick={() => void chooseFiles()}>Import files</Button>
      </>}>
        <p className="muted">Import XML files, .index files, a .zip{mobile ? "" : " or your whole custom content folder"}. A preview lists what will be added before you install it. <Button small variant="clear" onClick={notice.read}>Content import notice</Button></p>
        <input ref={fileRef} type="file" multiple accept={mobile ? `${CONTENT_ACCEPT},${UNTYPED_FILE}` : CONTENT_ACCEPT} hidden aria-label="Choose content files" onChange={(e) => void onFiles(e.target.files)} />
      </Section>
      <Section title="Installed packs">
        <ul className="list" aria-label="Content packs">
          {(packs.data ?? []).map((p) => (
            <li key={p.id}>
              <div className="v4-pack-row">
                <CheckBox aria-label={`Enable ${p.name}`} checked={p.enabled} onChange={(e) => void api.setPackEnabled({ packId: p.id, enabled: e.target.checked }).then(packs.reload)} />
                <span style={{ flex: 1, minWidth: "10rem" }}>
                  <strong>{p.name}</strong> <span className="muted small">v{p.version}</span>
                  <span className="muted small" style={{ display: "block" }}>{p.kind} · {p.rulesets.map((r) => (r === "dnd2024" ? "2024" : "2014")).join(", ")} · {p.elementCount} elements · {p.license}</span>
                  {p.redistributable === false && <span className="chip warning">Local import — not redistributable</span>}
                  {p.attribution && <span className="muted small" style={{ display: "block" }}>{p.attribution}</span>}
                </span>
                {p.kind !== "bundled" && <Button small variant="danger" onClick={() => void remove(p)}>Remove</Button>}
              </div>
            </li>
          ))}
        </ul>
      </Section>
    </>
  );
}
