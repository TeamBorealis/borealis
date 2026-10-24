import { useState } from "react";
import { Button } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";
import { SectionDesc, SectionTitle } from "./rows";

/** Settings > About: the semantic search model's NOTICE and Apache-2.0 licence (EM6), bundled in the app. */
export function ModelLicence() {
  const api = useApi();
  const lic = useAsync(() => api.semanticModelLicense(), [api]);
  const [open, setOpen] = useState(false);
  if (!lic.data) return null;
  return (
    <>
      <SectionTitle>SEMANTIC SEARCH MODEL</SectionTitle>
      <SectionDesc>{lic.data.model} is downloaded only if you turn on semantic search in Borealis Settings &gt; Search.</SectionDesc>
      <pre className="aur-set-licence" aria-label={`${lic.data.model} notice`}>{lic.data.notice}</pre>
      <div className="aur-set-buttons">
        <Button aria-expanded={open} onClick={() => setOpen((o) => !o)}>{open ? "Hide licence" : "Show licence"}</Button>
      </div>
      {open && <pre className="aur-set-licence" aria-label={`${lic.data.model} licence`}>{lic.data.license}</pre>}
    </>
  );
}
