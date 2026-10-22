import type { ReactNode } from "react";
import { Navigate, Route, Routes } from "react-router";
import { ControlsGallery } from "./components/controls/ControlsGallery";
import { AppShell } from "./components/shell/AppShell";
import { GlobalImport } from "./components/GlobalImport";
import { ImportPage } from "./pages/import/ImportPage";
import { AiPage } from "./pages/ai/AiPage";
import { AiSetupPage } from "./pages/setup/AiSetupPage";
import { usePlatform } from "./state/platform";
import { BuilderPage } from "./pages/builder/BuilderPage";
import { CharactersPage } from "./pages/characters/CharactersPage";
import { CompendiumRoute } from "./pages/compendium/CompendiumFlyout";
import { ContentPage } from "./pages/content/ContentPage";
import { EquipmentPage } from "./pages/equipment/EquipmentPage";
import { ManagePage } from "./pages/manage/ManagePage";
import { MorePage } from "./pages/MorePage";
import { SourcesPage } from "./pages/content/SourcesPage";
import { SettingsRedirect } from "./pages/settings/SettingsHost";
import { SheetPage } from "./pages/sheet/SheetPage";
import { SpellsPage } from "./pages/spells/SpellsPage";
import { PlayPage } from "./pages/play/PlayPage";

/** Routes of the optional AI assistant: they redirect to the characters until it is on (and always on mobile). */
function AiOnly({ children }: { children: ReactNode }) {
  const { aiEnabled, ready } = usePlatform();
  if (!ready) return null;
  return aiEnabled ? <>{children}</> : <Navigate to="/characters" replace />;
}

export function App() {
  return (
    <AppShell>
      <GlobalImport />
      <Routes>
        <Route path="/" element={<Navigate to="/characters" replace />} />
        <Route path="/characters" element={<CharactersPage />} />
        <Route path="/builder/:tab?" element={<BuilderPage />} />
        <Route path="/play" element={<PlayPage />} />
        <Route path="/spells" element={<SpellsPage />} />
        <Route path="/equipment" element={<EquipmentPage />} />
        <Route path="/manage" element={<ManagePage />} />
        <Route path="/sheet" element={<SheetPage />} />
        <Route path="/compendium" element={<CompendiumRoute />} />
        <Route path="/content" element={<ContentPage />} />
        <Route path="/content/sources" element={<SourcesPage />} />
        <Route path="/import" element={<AiOnly><ImportPage /></AiOnly>} />
        <Route path="/ai" element={<AiOnly><AiPage /></AiOnly>} />
        <Route path="/setup/ai" element={<AiOnly><AiSetupPage /></AiOnly>} />
        <Route path="/settings" element={<SettingsRedirect />} />
        <Route path="/more" element={<MorePage />} />
      {import.meta.env.DEV && <Route path="/dev/controls" element={<ControlsGallery />} />}
        <Route path="*" element={<Navigate to="/characters" replace />} />
      </Routes>
    </AppShell>
  );
}
