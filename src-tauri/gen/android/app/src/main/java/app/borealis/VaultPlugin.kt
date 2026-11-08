package app.borealis

import android.app.Activity
import android.content.ContentResolver
import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document
import android.util.Base64
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.ByteArrayOutputStream
import java.io.FileNotFoundException
import java.io.FileOutputStream
import java.io.InputStream
import java.util.UUID
import java.util.concurrent.Executors
import org.json.JSONObject

@InvokeArg
class VaultTreeArgs {
  lateinit var tree: String
}

@InvokeArg
class VaultPathArgs {
  lateinit var tree: String
  var path: String = ""
}

@InvokeArg
class VaultReadArgs {
  lateinit var tree: String
  lateinit var path: String
  var max: Long = 0
}

@InvokeArg
class VaultWriteArgs {
  lateinit var tree: String
  lateinit var path: String
  var data: String = ""
}

@InvokeArg
class VaultRenameArgs {
  lateinit var tree: String
  lateinit var path: String
  lateinit var name: String
}

/** A failure the backend tells apart (`vault_android.rs` maps the codes). */
class VaultFailure(val code: String, message: String) : Exception(message)

/** One document of the tree. */
data class VaultDoc(val id: String, val name: String, val dir: Boolean, val size: Long, val mtime: Long)

/**
 * The Obsidian vault on Android (B-221, decision U17b): a Storage Access Framework document tree. The backend
 * (`vault_android.rs`, `vault/tree_store.rs`) names files by tree-relative `/` paths and keeps the notes view's rules;
 * this plugin only does the framework calls, on its own thread so the UI thread never waits on storage.
 *
 * - `pickTree` opens ACTION_OPEN_DOCUMENT_TREE and persists read and write permission, so the vault survives restarts.
 * - `write` cannot replace a document atomically (the framework has no rename-over): the new bytes go first to a hidden
 *   temporary document next to the note, then the note is rewritten in place ("wt", truncated), read back and compared,
 *   and only then is the temporary copy deleted. If anything fails, the full new text stays in that hidden copy.
 * - `create` and `rename` never replace anything: a taken name is refused, and a provider that renames a new document
 *   on its own (`Note (1).md`) has that document removed again.
 */
@TauriPlugin
class VaultPlugin(private val activity: Activity) : Plugin(activity) {
  private val io = Executors.newSingleThreadExecutor()
  private val resolver: ContentResolver get() = activity.contentResolver

  /** Run `work` on the storage thread; a [VaultFailure] becomes a coded reject. */
  private fun background(invoke: Invoke, work: () -> JSObject) {
    io.execute {
      try {
        invoke.resolve(work())
      } catch (e: VaultFailure) {
        invoke.reject(e.message, e.code)
      } catch (e: FileNotFoundException) {
        invoke.reject(e.message ?: "not found", "notFound")
      } catch (e: SecurityException) {
        invoke.reject("Borealis no longer has access to that folder.", "noPermission")
      } catch (e: Exception) {
        invoke.reject(e.message ?: e.javaClass.simpleName)
      }
    }
  }

  @Command
  fun pickTree(invoke: Invoke) {
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(
      Intent.FLAG_GRANT_READ_URI_PERMISSION or
        Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
        Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
        Intent.FLAG_GRANT_PREFIX_URI_PERMISSION,
    )
    startActivityForResult(invoke, intent, "onTreePicked")
  }

  @ActivityCallback
  fun onTreePicked(invoke: Invoke, result: ActivityResult) {
    val uri = result.data?.data
    if (result.resultCode != Activity.RESULT_OK || uri == null) {
      invoke.resolve(JSObject().put("tree", JSONObject.NULL))
      return
    }
    try {
      resolver.takePersistableUriPermission(
        uri,
        Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
      )
      invoke.resolve(JSObject().put("tree", uri.toString()))
    } catch (e: SecurityException) {
      invoke.reject("Android did not let Borealis keep access to that folder.", "noPermission")
    }
  }

  /** The tree is still granted (read and write) and its folder answers. */
  @Command
  fun check(invoke: Invoke) {
    val args = invoke.parseArgs(VaultTreeArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val granted = resolver.persistedUriPermissions.any {
        it.uri == tree && it.isReadPermission && it.isWritePermission
      }
      if (!granted) throw VaultFailure("noPermission", "The folder permission is gone.")
      root(tree)
      JSObject()
    }
  }

  @Command
  fun release(invoke: Invoke) {
    val args = invoke.parseArgs(VaultTreeArgs::class.java)
    background(invoke) {
      try {
        resolver.releasePersistableUriPermission(
          Uri.parse(args.tree),
          Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
        )
      } catch (e: SecurityException) {
        // Not held any more: nothing to give back.
      }
      JSObject()
    }
  }

  /** Every entry of the tree; hidden folders are listed but not entered. */
  @Command
  fun list(invoke: Invoke) {
    val args = invoke.parseArgs(VaultTreeArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val out = JSArray()
      val pending = ArrayDeque<Pair<String, String>>()
      pending.add("" to root(tree).id)
      var count = 0
      while (pending.isNotEmpty() && count < VaultPaths.MAX_LISTED) {
        val (dir, id) = pending.removeFirst()
        for (doc in children(tree, id)) {
          val path = VaultPaths.join(dir, doc.name)
          out.put(entry(path, doc))
          count++
          if (doc.dir && !VaultPaths.isHidden(doc.name)) pending.add(path to doc.id)
        }
      }
      JSObject().put("entries", out)
    }
  }

  @Command
  fun children(invoke: Invoke) {
    val args = invoke.parseArgs(VaultPathArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val dir = if (args.path.isEmpty()) root(tree) else find(tree, args.path)
      if (!dir.dir) throw VaultFailure("notAFolder", "not a folder")
      val out = JSArray()
      children(tree, dir.id).forEach { out.put(entry(VaultPaths.join(args.path, it.name), it)) }
      JSObject().put("entries", out)
    }
  }

  @Command
  fun stat(invoke: Invoke) {
    val args = invoke.parseArgs(VaultPathArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val doc = findOrNull(tree, args.path)
      JSObject().put("entry", doc?.let { entry(args.path, it) } ?: JSONObject.NULL)
    }
  }

  @Command
  fun read(invoke: Invoke) {
    val args = invoke.parseArgs(VaultReadArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val doc = find(tree, args.path)
      if (doc.dir) throw VaultFailure("notAFolder", "a folder")
      val bytes = readCapped(docUri(tree, doc.id), args.max)
      JSObject().put("data", bytes?.let { Base64.encodeToString(it, Base64.NO_WRAP) } ?: JSONObject.NULL)
    }
  }

  @Command
  fun write(invoke: Invoke) {
    val args = invoke.parseArgs(VaultWriteArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val bytes = Base64.decode(args.data, Base64.NO_WRAP)
      val doc = find(tree, args.path)
      if (doc.dir) throw VaultFailure("notAFolder", "a folder")
      val parent = if (VaultPaths.parent(args.path).isEmpty()) root(tree) else find(tree, VaultPaths.parent(args.path))
      // 1. The new text, safe in a hidden copy next to the note.
      val spare = createDoc(tree, parent.id, VaultPaths.spareName(UUID.randomUUID().toString()), false)
      writeBytes(spare, bytes)
      // 2. The note rewritten in place, then checked.
      val target = docUri(tree, doc.id)
      writeBytes(target, bytes)
      val back = readCapped(target, bytes.size.toLong())
      if (back == null || !back.contentEquals(bytes)) {
        throw VaultFailure("failed", "The note could not be written exactly; the new text is kept in a hidden copy.")
      }
      // 3. Done: the copy goes.
      DocumentsContract.deleteDocument(resolver, spare)
      JSObject()
    }
  }

  @Command
  fun create(invoke: Invoke) {
    val args = invoke.parseArgs(VaultWriteArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val bytes = Base64.decode(args.data, Base64.NO_WRAP)
      var folder = root(tree)
      for (part in VaultPaths.folders(args.path)) {
        val next = children(tree, folder.id).firstOrNull { it.name == part }
        folder = when {
          next == null -> {
            val made = createDoc(tree, folder.id, part, true)
            VaultDoc(DocumentsContract.getDocumentId(made), part, true, 0, 0)
          }
          !next.dir -> throw VaultFailure("notAFolder", "a file is in the way")
          else -> next
        }
      }
      val name = VaultPaths.name(args.path)
      if (children(tree, folder.id).any { it.name == name }) throw VaultFailure("exists", "taken")
      val made = createDoc(tree, folder.id, name, false)
      writeBytes(made, bytes)
      JSObject()
    }
  }

  @Command
  fun rename(invoke: Invoke) {
    val args = invoke.parseArgs(VaultRenameArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val doc = find(tree, args.path)
      val parentPath = VaultPaths.parent(args.path)
      val parent = if (parentPath.isEmpty()) root(tree) else find(tree, parentPath)
      val siblings = children(tree, parent.id)
      if (siblings.any { it.name == args.name }) throw VaultFailure("exists", "taken")
      var uri = docUri(tree, doc.id)
      // A new case of the same name: through a hidden name first, for providers that ignore case.
      if (doc.name.equals(args.name, ignoreCase = true)) {
        uri = renameExactly(uri, VaultPaths.spareName(UUID.randomUUID().toString()))
      }
      renameExactly(uri, args.name)
      JSObject()
    }
  }

  @Command
  fun delete(invoke: Invoke) {
    val args = invoke.parseArgs(VaultPathArgs::class.java)
    background(invoke) {
      val tree = Uri.parse(args.tree)
      val doc = find(tree, args.path)
      if (doc.dir) throw VaultFailure("notAFolder", "a folder")
      if (!DocumentsContract.deleteDocument(resolver, docUri(tree, doc.id))) {
        throw VaultFailure("failed", "The file could not be deleted.")
      }
      JSObject()
    }
  }

  // --- Storage Access Framework helpers -------------------------------------------------------------------------

  private fun docUri(tree: Uri, id: String): Uri = DocumentsContract.buildDocumentUriUsingTree(tree, id)

  private fun doc(cursor: Cursor): VaultDoc = VaultDoc(
    id = cursor.getString(0),
    name = cursor.getString(1) ?: "",
    dir = cursor.getString(2) == Document.MIME_TYPE_DIR,
    size = if (cursor.isNull(3)) 0 else cursor.getLong(3),
    mtime = if (cursor.isNull(4)) 0 else cursor.getLong(4),
  )

  private fun root(tree: Uri): VaultDoc {
    val id = DocumentsContract.getTreeDocumentId(tree)
    resolver.query(docUri(tree, id), COLUMNS, null, null, null)?.use { c ->
      if (c.moveToFirst()) return doc(c)
    }
    throw VaultFailure("noPermission", "The folder cannot be read.")
  }

  private fun children(tree: Uri, id: String): List<VaultDoc> {
    val uri = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id)
    val out = mutableListOf<VaultDoc>()
    resolver.query(uri, COLUMNS, null, null, null)?.use { c ->
      while (c.moveToNext()) out.add(doc(c))
    }
    return out
  }

  /** The document at `path`, by exact names from the tree's root; null when nothing is there. */
  private fun findOrNull(tree: Uri, path: String): VaultDoc? {
    var current = root(tree)
    for (part in VaultPaths.parts(path)) {
      if (!current.dir) return null
      current = children(tree, current.id).firstOrNull { it.name == part } ?: return null
    }
    return current
  }

  private fun find(tree: Uri, path: String): VaultDoc =
    findOrNull(tree, path) ?: throw VaultFailure("notFound", "not found")

  /** A new document named exactly `name` in the folder `parentId`; a provider that picks another name is undone. */
  private fun createDoc(tree: Uri, parentId: String, name: String, dir: Boolean): Uri {
    val mime = if (dir) Document.MIME_TYPE_DIR else VaultPaths.MIME_ANY
    val made = DocumentsContract.createDocument(resolver, docUri(tree, parentId), mime, name)
      ?: throw VaultFailure("failed", "The file could not be created.")
    val actual = nameOf(made)
    if (actual != name) {
      DocumentsContract.deleteDocument(resolver, made)
      throw VaultFailure("exists", "taken")
    }
    return made
  }

  private fun renameExactly(uri: Uri, name: String): Uri {
    val renamed = DocumentsContract.renameDocument(resolver, uri, name)
      ?: throw VaultFailure("failed", "The file could not be renamed.")
    if (nameOf(renamed) != name) throw VaultFailure("exists", "taken")
    return renamed
  }

  private fun nameOf(uri: Uri): String? =
    resolver.query(uri, arrayOf(Document.COLUMN_DISPLAY_NAME), null, null, null)?.use { c ->
      if (c.moveToFirst()) c.getString(0) else null
    }

  /** Write `bytes` over the document, truncating it, and flush it to storage. */
  private fun writeBytes(uri: Uri, bytes: ByteArray) {
    val pfd = try {
      resolver.openFileDescriptor(uri, "wt")
    } catch (e: IllegalArgumentException) {
      // Providers without "wt": "w" and an explicit truncate below.
      resolver.openFileDescriptor(uri, "w")
    } ?: throw VaultFailure("failed", "The file could not be opened.")
    pfd.use {
      FileOutputStream(it.fileDescriptor).use { out ->
        out.write(bytes)
        out.flush()
        try {
          out.channel.truncate(bytes.size.toLong())
        } catch (e: Exception) {
          // Not a seekable file (a provider pipe): the read-back check decides.
        }
        try {
          out.fd.sync()
        } catch (e: Exception) {
          // Not every provider's descriptor can sync.
        }
      }
    }
  }

  /** At most `max` bytes; null when there are more. */
  private fun readCapped(uri: Uri, max: Long): ByteArray? {
    val input: InputStream = resolver.openInputStream(uri)
      ?: throw VaultFailure("failed", "The file could not be opened.")
    input.use {
      val out = ByteArrayOutputStream()
      val buf = ByteArray(64 * 1024)
      var total = 0L
      while (true) {
        val n = it.read(buf)
        if (n < 0) break
        total += n
        if (total > max) return null
        out.write(buf, 0, n)
      }
      return out.toByteArray()
    }
  }

  private fun entry(path: String, doc: VaultDoc): JSObject = JSObject()
    .put("path", path)
    .put("dir", doc.dir)
    .put("size", if (doc.dir) 0 else doc.size)
    .put("mtimeMs", doc.mtime)

  companion object {
    private val COLUMNS = arrayOf(
      Document.COLUMN_DOCUMENT_ID,
      Document.COLUMN_DISPLAY_NAME,
      Document.COLUMN_MIME_TYPE,
      Document.COLUMN_SIZE,
      Document.COLUMN_LAST_MODIFIED,
    )
  }
}

/** Path rules shared by the plugin's commands (pure, unit-tested in `VaultPathsTest`). */
object VaultPaths {
  /** Most entries listed (the backend lists at most 20 000 visible ones). */
  const val MAX_LISTED = 50_000

  /** Files are created with no type, so no provider adds an extension of its own. */
  const val MIME_ANY = "application/octet-stream"

  /** Obsidian, sync services and the backend all skip names starting with a dot. */
  fun isHidden(name: String): Boolean = name.startsWith(".")

  fun parts(path: String): List<String> = path.split('/').filter { it.isNotEmpty() }

  fun folders(path: String): List<String> = parts(path).dropLast(1)

  fun name(path: String): String = parts(path).lastOrNull() ?: ""

  fun parent(path: String): String = folders(path).joinToString("/")

  fun join(dir: String, name: String): String = if (dir.isEmpty()) name else "$dir/$name"

  /** The hidden copy a write keeps until the note is safely rewritten. */
  fun spareName(id: String): String = ".borealis-$id.tmp"
}
