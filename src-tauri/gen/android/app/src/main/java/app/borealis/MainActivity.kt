package app.borealis

import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.util.Base64
import android.util.Log
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
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
  private var lastNight: Boolean? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    lastNight = SystemTheme.isNight(resources.configuration.uiMode)
    handleIntent(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    handleIntent(intent)
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    super.onWebViewCreate(webView)
    webView.addJavascriptInterface(InsetsBridge(), "BorealisInsets")
    webView.addJavascriptInterface(
      SystemBarsBridge({ runOnUiThread(it) }, { SystemBars.apply(this, it) }, { SystemTheme.isNight(resources.configuration.uiMode) }),
      "BorealisSystemBars",
    )
    ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom
      val json = SystemInsets.json(bars.top, bars.right, bars.bottom, bars.left, resources.displayMetrics.density, ime)
      SystemInsets.update(json)
      view.post { webView.evaluateJavascript(SystemInsets.applyScript(json), null) }
      // Keep the WebView's own handling (safe-area env(), keyboard resize).
      ViewCompat.onApplyWindowInsets(view, insets)
    }
  }

  /** uiMode is in configChanges, so a dark-mode switch lands here: the page re-reads it (B-48 O9). */
  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    val night = SystemTheme.isNight(newConfig.uiMode)
    if (night == lastNight) return
    lastNight = night
    val view = webView ?: return
    view.post { view.evaluateJavascript(SystemTheme.CHANGED_SCRIPT, null) }
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
    val own = ownAuthorities()
    for (uri in uris) {
      val why = OpenedUriRules.refusal(uri.scheme, uri.authority, own)
      if (why != null) {
        Log.w(TAG, "not opening a shared file: $why")
        continue
      }
      // Streamed with the cap: an oversized or endless stream is dropped, never buffered whole.
      val bytes = try {
        contentResolver.openInputStream(uri)?.use { OpenedUriRules.readCapped(it) }
      } catch (e: Exception) { null } ?: continue
      if (bytes.isEmpty()) continue
      OpenedFiles.add(displayName(uri), Base64.encodeToString(bytes, Base64.NO_WRAP))
      added = true
    }
    if (added) nudge()
  }

  /** Authorities of this app's own content providers (FileProvider, library providers). */
  private fun ownAuthorities(): Set<String> {
    val declared = try {
      @Suppress("DEPRECATION")
      packageManager.getPackageInfo(packageName, PackageManager.GET_PROVIDERS).providers
        ?.flatMap { (it.authority ?: "").split(';') }
        ?.filter { it.isNotEmpty() }
        .orEmpty()
    } catch (e: Exception) { emptyList() }
    return (declared + "$packageName.fileprovider").toSet()
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
    private const val TAG = "Borealis"
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
