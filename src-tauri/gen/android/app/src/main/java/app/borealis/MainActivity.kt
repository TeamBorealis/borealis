package app.borealis

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.util.Base64
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import org.json.JSONObject

/**
 * Hands files opened with Borealis ("Open with" / Share) to the web UI as `borealis-open-file` events.
 * Files that arrive before the page is ready are queued in `window.__borealisOpened`, which the UI drains.
 */
class MainActivity : TauriActivity() {
  private var webView: WebView? = null
  private val pending = mutableListOf<String>()

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    handleIntent(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    handleIntent(intent)
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    super.onWebViewCreate(webView)
  }

  private fun handleIntent(intent: Intent?) {
    val uris = mutableListOf<Uri>()
    when (intent?.action) {
      Intent.ACTION_VIEW -> intent.data?.let { uris.add(it) }
      Intent.ACTION_SEND -> @Suppress("DEPRECATION")
        (intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM))?.let { uris.add(it) }
      Intent.ACTION_SEND_MULTIPLE -> @Suppress("DEPRECATION")
        intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)?.let { uris.addAll(it) }
    }
    for (uri in uris) {
      val bytes = try {
        contentResolver.openInputStream(uri)?.use { it.readBytes() }
      } catch (e: Exception) { null } ?: continue
      if (bytes.isEmpty() || bytes.size > MAX_OPEN_BYTES) continue
      val name = uri.lastPathSegment?.substringAfterLast('/') ?: "file"
      val json = JSONObject()
        .put("fileName", name)
        .put("contentBase64", Base64.encodeToString(bytes, Base64.NO_WRAP))
        .toString()
      deliver(json)
    }
  }

  private fun deliver(json: String) {
    val script = "(window.__borealisOpened = window.__borealisOpened || []).push($json);" +
      "window.dispatchEvent(new CustomEvent('borealis-open-file'));"
    val view = webView
    if (view == null) pending.add(script) else view.post { view.evaluateJavascript(script, null) }
  }

  override fun onResume() {
    super.onResume()
    val view = webView ?: return
    pending.forEach { s -> view.post { view.evaluateJavascript(s, null) } }
    pending.clear()
  }

  companion object {
    private const val MAX_OPEN_BYTES = 64 * 1024 * 1024
  }
}
