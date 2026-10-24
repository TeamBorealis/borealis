package app.borealis

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class ShareTextArgs {
  lateinit var text: String
  lateinit var fileName: String
  var title: String = "Borealis"
}

@InvokeArg
class ShareFileArgs {
  lateinit var path: String
  lateinit var fileName: String
  var mime: String = "application/octet-stream"
  var title: String = "Borealis"
}

/**
 * "Share logs" on Android (B-40): the System WebView has no Web Share API, so the backend
 * (`native_share.rs`) calls `shareText`. The text is written to `cache/share/<fileName>` and sent
 * with ACTION_SEND through the app's FileProvider (`res/xml/file_paths.xml` cache-path), with a
 * read grant for the receiving app only. It also hands the backend the files the OS opened with
 * Borealis (`takeOpenedFiles`, B-24).
 */
@TauriPlugin
class SharePlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun shareText(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ShareTextArgs::class.java)
      val dir = File(activity.cacheDir, SHARE_DIR).apply { mkdirs() }
      val file = File(dir, File(args.fileName).name)
      file.writeText(args.text)
      val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
      val send = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_STREAM, uri)
        putExtra(Intent.EXTRA_SUBJECT, args.title)
        clipData = ClipData.newRawUri(args.title, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      activity.runOnUiThread {
        try {
          activity.startActivity(Intent.createChooser(send, args.title))
          invoke.resolve()
        } catch (e: Exception) {
          invoke.reject(e.message ?: "The share sheet could not open.")
        }
      }
    } catch (e: Exception) {
      invoke.reject(e.message ?: "The share sheet could not open.")
    }
  }

  /**
   * "Send as a file" (A2): the backend built a `.borealis` bundle in the app's data folder; copy it to
   * `cache/share/<fileName>` (the only folder the FileProvider serves), clear older copies, and send it
   * with ACTION_SEND and a read grant for the receiving app only.
   */
  @Command
  fun shareFile(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ShareFileArgs::class.java)
      val dir = File(activity.cacheDir, SHARE_DIR).apply { mkdirs() }
      dir.listFiles()?.forEach { it.delete() }
      val file = File(dir, File(args.fileName).name)
      File(args.path).copyTo(file, overwrite = true)
      val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
      val send = Intent(Intent.ACTION_SEND).apply {
        type = args.mime
        putExtra(Intent.EXTRA_STREAM, uri)
        putExtra(Intent.EXTRA_SUBJECT, args.title)
        clipData = ClipData.newRawUri(args.title, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      activity.runOnUiThread {
        try {
          activity.startActivity(Intent.createChooser(send, args.title))
          invoke.resolve()
        } catch (e: Exception) {
          invoke.reject(e.message ?: "The share sheet could not open.")
        }
      }
    } catch (e: Exception) {
      invoke.reject(e.message ?: "The share sheet could not open.")
    }
  }

  /** Files opened with Borealis since the last call (see `MainActivity`), as `{ files: [...] }`. */
  @Command
  fun takeOpenedFiles(invoke: Invoke) {
    invoke.resolve(JSObject().put("files", OpenedFiles.take()))
  }

  companion object {
    private const val SHARE_DIR = "share"
  }
}
