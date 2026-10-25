package app.borealis

import android.webkit.JavascriptInterface
import kotlin.math.roundToInt

/**
 * The status bar, navigation bar and display cutout, in CSS px, for the page's safe-area variables
 * (`--sys-inset-*`, read by `--safe-*` in tokens.css). The app is edge to edge, but the WebView's
 * `env(safe-area-inset-*)` only reports the display cutout: in landscape the cutout moves to a side
 * and the top inset becomes 0 while the status bar still covers the title bar (HW1 A2).
 * MainActivity pushes every change; the page also asks [InsetsBridge] once when it starts.
 */
object SystemInsets {
  @Volatile private var latest = "{}"

  /** JSON `{"t","r","b","l"}` in CSS px (device px / density), never negative. */
  fun json(top: Int, right: Int, bottom: Int, left: Int, density: Float): String {
    val d = if (density > 0f) density else 1f
    fun css(px: Int) = (px.coerceAtLeast(0) / d).roundToInt()
    return "{\"t\":${css(top)},\"r\":${css(right)},\"b\":${css(bottom)},\"l\":${css(left)}}"
  }

  /** Script that hands `json` to the page (a no-op until the page installed its hook). */
  fun applyScript(json: String): String =
    "window.__borealisApplyInsets&&window.__borealisApplyInsets($json)"

  fun update(json: String) {
    latest = json
  }

  fun current(): String = latest
}

/** `window.BorealisInsets.get()`: the latest insets JSON (numbers only). */
class InsetsBridge {
  @JavascriptInterface
  fun get(): String = SystemInsets.current()
}
