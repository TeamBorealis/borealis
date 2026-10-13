import { FA5_HEARTBEAT } from "../controls/openIcons";
import { BrandMark } from "../shell/BrandMark";
import type { StartupProgress } from "../../lib/startup";

interface Props {
  progress: StartupProgress | null;
  version: string;
  /** Fills the screen (mobile overlay) instead of the fixed 650 x 350 window panel. */
  fullscreen?: boolean;
  /** Fading out after startup finished. */
  leaving?: boolean;
  /** Closes the app from the error state (desktop only). */
  onQuit?: () => void;
}

const START: StartupProgress = { message: "Starting", percent: 0, failed: false, mobile: false };

/** Original aurora ribbons (CSS/SVG written for Borealis; no third-party imagery). */
function Backdrop() {
  return (
    <svg className="splash-ribbons" viewBox="0 0 650 350" preserveAspectRatio="xMidYMid slice" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id="splash-g1" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor="#2f875b" stopOpacity="0" />
          <stop offset="0.35" stopColor="#6fd0b5" stopOpacity="0.55" />
          <stop offset="0.75" stopColor="#4a7bf0" stopOpacity="0.35" />
          <stop offset="1" stopColor="#4a7bf0" stopOpacity="0" />
        </linearGradient>
        <linearGradient id="splash-g2" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor="#305754" stopOpacity="0" />
          <stop offset="0.5" stopColor="#37753d" stopOpacity="0.5" />
          <stop offset="1" stopColor="#6fd0b5" stopOpacity="0" />
        </linearGradient>
        <linearGradient id="splash-g3" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor="#8b6fd0" stopOpacity="0" />
          <stop offset="0.6" stopColor="#8b6fd0" stopOpacity="0.28" />
          <stop offset="1" stopColor="#6fd0b5" stopOpacity="0" />
        </linearGradient>
        <filter id="splash-blur" x="-20%" y="-50%" width="140%" height="200%"><feGaussianBlur stdDeviation="16" /></filter>
      </defs>
      <g filter="url(#splash-blur)">
        <path className="splash-rib splash-rib-1" d="M-40 120 C 120 20 240 190 380 90 S 600 10 700 80 L700 150 C 580 90 460 200 340 160 S 120 90 -40 190 Z" fill="url(#splash-g1)" />
        <path className="splash-rib splash-rib-2" d="M-40 210 C 100 130 230 270 370 190 S 590 150 700 210 L700 262 C 560 210 470 300 340 270 S 110 200 -40 290 Z" fill="url(#splash-g2)" />
        <path className="splash-rib splash-rib-3" d="M-40 40 C 150 -10 260 80 410 30 S 590 -20 700 20 L700 60 C 570 30 450 110 330 80 S 130 30 -40 90 Z" fill="url(#splash-g3)" />
      </g>
    </svg>
  );
}

function Heart() {
  return (
    <svg className="splash-heart" viewBox={FA5_HEARTBEAT.viewBox} width="14" height="14" aria-hidden="true" focusable="false">
      <path d={FA5_HEARTBEAT.d} fill="currentColor" />
    </svg>
  );
}

function ErrorPanel({ progress, version, onQuit }: { progress: StartupProgress; version: string; onQuit?: () => void }) {
  return (
    <div className="splash-error" role="alert">
      <h2 className="splash-error-title">Borealis could not start</h2>
      <p className="splash-error-what">{progress.message}</p>
      {progress.details && (
        <details className="splash-error-details">
          <summary>Additional details</summary>
          <pre>{progress.details}</pre>
        </details>
      )}
      <div className="splash-error-foot">
        <span className="splash-version">Version {version}</span>
        {onQuit && <button type="button" className="splash-quit" onClick={onQuit}>Close</button>}
      </div>
    </div>
  );
}

/** The startup splash: mark, wordmark, tagline, status line with heartbeat glyph and progress bar. */
export function SplashView({ progress, version, fullscreen = false, leaving = false, onQuit }: Props) {
  const p = progress ?? START;
  const pct = Math.max(0, Math.min(100, Math.round(p.percent)));
  const cls = `splash ${fullscreen ? "splash--full" : "splash--window"}${leaving ? " splash--leaving" : ""}`;
  return (
    <div className={cls} data-testid="splash" data-failed={p.failed ? "true" : "false"}>
      <Backdrop />
      <div className="splash-brand">
        <BrandMark className="splash-mark" />
        <div className="splash-wordmark">Borealis</div>
        <div className="splash-tagline">Character builder for fifth edition</div>
      </div>
      {p.failed ? (
        <ErrorPanel progress={p} version={version} onQuit={onQuit} />
      ) : (
        <div className="splash-status">
          <div className="splash-line">
            <Heart />
            <span className="splash-message" role="status" aria-live="polite">{p.message}</span>
            <span className="splash-percent">{pct}%</span>
          </div>
          <div className="splash-bar" role="progressbar" aria-label="Startup progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
            <div className="splash-bar-fill" style={{ width: `${pct}%` }} />
          </div>
          <span className="splash-version">{version}</span>
        </div>
      )}
    </div>
  );
}
