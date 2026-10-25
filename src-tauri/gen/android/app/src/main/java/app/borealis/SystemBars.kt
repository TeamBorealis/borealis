package app.borealis

import android.content.res.Configuration
import android.graphics.Color
import android.webkit.JavascriptInterface
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge

/**
 * B-48 (mobile-facelift-spec sec. 5.9, O9): the status and navigation bar icons follow the app's theme, not only the
 * phone's. `enableEdgeToEdge()` in `onCreate` picks them from the system setting once, and `uiMode` is in the
 * activity's `configChanges`, so neither a live dark-mode switch nor a Settings > Appearance choice that differs from
 * the system would repaint them. The page reports its effective theme through [SystemBarsBridge] whenever it applies it.
 */
object SystemBars {
  /** androidx.activity's default navigation bar scrims for three-button navigation (gesture navigation has none). */
  private val LIGHT_SCRIM = Color.argb(0xe6, 0xFF, 0xFF, 0xFF)
  private val DARK_SCRIM = Color.argb(0x80, 0x1b, 0x1b, 0x1b)

  /** Dark icons over a light page, light icons over a dark one; both bars stay transparent over the app background. */
  fun apply(activity: ComponentActivity, lightPage: Boolean) {
    val status = if (lightPage) {
      SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
    } else {
      SystemBarStyle.dark(Color.TRANSPARENT)
    }
    val navigation = if (lightPage) SystemBarStyle.light(LIGHT_SCRIM, DARK_SCRIM) else SystemBarStyle.dark(DARK_SCRIM)
    activity.enableEdgeToEdge(status, navigation)
  }
}

/**
 * The phone's dark mode for the page's "System" theme (O9). The Android System WebView keeps answering
 * `prefers-color-scheme: light` with the phone in dark mode, so the page asks [SystemBarsBridge.isSystemNight], and
 * MainActivity sends [CHANGED_SCRIPT] when the setting changes (uiMode changes do not recreate the activity).
 */
object SystemTheme {
  fun isNight(uiMode: Int): Boolean = (uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

  const val CHANGED_SCRIPT = "window.dispatchEvent(new CustomEvent('borealis-system-theme'))"
}

/**
 * `window.BorealisSystemBars` for the page (`ui/src/lib/systemBars.ts`), called on the WebView's JavaBridge thread.
 * `setLight(light)` runs [apply] on the UI thread through [post]; its only input is a boolean. `isSystemNight()` answers
 * the phone's dark mode from [night] (read only).
 */
class SystemBarsBridge(
  private val post: (Runnable) -> Unit,
  private val apply: (Boolean) -> Unit,
  private val night: () -> Boolean,
) {
  @JavascriptInterface
  fun setLight(light: Boolean) {
    post(Runnable { apply(light) })
  }

  @JavascriptInterface
  fun isSystemNight(): Boolean = night()
}
