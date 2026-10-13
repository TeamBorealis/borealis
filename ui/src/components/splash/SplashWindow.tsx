import type { StartupBridge } from "../../lib/startup";
import { SplashView } from "./SplashView";
import { useStartup } from "./useStartup";

/** Root of the desktop splash window (`index.html#/splash`). The backend closes it when startup is done. */
export function SplashWindow({ bridge, version }: { bridge: StartupBridge; version: string }) {
  const { progress } = useStartup(bridge, "splash-ready");
  return <SplashView progress={progress} version={version} onQuit={bridge.quit?.bind(bridge)} />;
}
