package app.borealis

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.util.Base64
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import org.json.JSONArray
import org.json.JSONObject

/**
 * Files opened with Borealis ("Open with" / Share). Each file is read once and parked in [OpenedFiles];
 * the UI takes them through the backend (`take_opened_files` -> `SharePlugin.takeOpenedFiles`) when it
 * mounts and whenever a `borealis-open-file` event nudges it. Pulling instead of pushing keeps a cold
 * start working: the file arrives before the page exists, and a script sent then would be lost.
 */
class MainActivity : TauriActivity() {
  private var webView: WebView? = null

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
    var added = false
    for (uri in uris) {
      val bytes = try {
        contentResolver.openInputStream(uri)?.use { it.readBytes() }
      } catch (e: Exception) { null } ?: continue
      if (bytes.isEmpty() || bytes.size > MAX_OPEN_BYTES) continue
      OpenedFiles.add(displayName(uri), Base64.encodeToString(bytes, Base64.NO_WRAP))
      added = true
    }
    if (added) nudge()
  }

  /** The file's own name: providers such as Downloads use opaque ids (`msf:123`) in the URI. */
  private fun displayName(uri: Uri): String {
    val fromProvider = try {
      contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
        if (c.moveToFirst()) c.getString(0) else null
      }
    } catch (e: Exception) { null }
    return fromProvider?.takeIf { it.isNotBlank() }
      ?: uri.lastPathSegment?.substringAfterLast('/')
      ?: "file"
  }

  /** A running page drains at once; before the page exists the UI drains on mount anyway. */
  private fun nudge() {
    val view = webView ?: return
    view.post { view.evaluateJavascript("window.dispatchEvent(new CustomEvent('borealis-open-file'));", null) }
  }

  companion object {
    private const val MAX_OPEN_BYTES = 64 * 1024 * 1024
  }
}

/** Files waiting for the UI, oldest first. Touched from the main thread and plugin calls. */
object OpenedFiles {
  private val queue = mutableListOf<JSONObject>()

  @Synchronized
  fun add(fileName: String, contentBase64: String) {
    queue.add(JSONObject().put("fileName", fileName).put("contentBase64", contentBase64))
  }

  @Synchronized
  fun take(): JSONArray {
    val out = JSONArray(queue)
    queue.clear()
    return out
  }
}
