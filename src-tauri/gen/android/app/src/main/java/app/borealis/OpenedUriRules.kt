package app.borealis

import java.io.ByteArrayOutputStream
import java.io.InputStream

/**
 * Which "Open with" / Share URIs [MainActivity] reads, and how much of them (security review Z).
 * Plain Kotlin without Android types, so the rules are checked on the JVM (`OpenedUriRulesTest`
 * under `app/src/test`; `open_with_manifests.rs` checks MainActivity goes through them).
 */
object OpenedUriRules {
  /** Largest file read into memory: the bundle reader's per-file cap (borealis-bundle `max_file_bytes`). */
  const val MAX_OPEN_BYTES: Int = 64 * 1024 * 1024

  private const val CHUNK_BYTES: Int = 64 * 1024

  /** Why a shared or opened file is not read: [why] for the log, [message] for the user (H5wn8: never silent). */
  enum class Refusal(val why: String) {
    NOT_CONTENT("only content URIs are opened"),
    NO_PROVIDER("the URI names no provider"),
    OWN_FILE("the URI points into Borealis itself"),
    TOO_LARGE("the file is larger than the cap"),
    UNREADABLE("the file could not be read"),
    EMPTY("the file is empty"),
  }

  /**
   * Null when the URI may be read, otherwise why not. Only `content:` URIs of another app's provider
   * are read: Borealis opens a URI with its own permissions, so a `file:` URI (or a URI of one of
   * Borealis's own providers) would let another app make it read its private files.
   */
  fun refusal(scheme: String?, authority: String?, ownAuthorities: Set<String>): Refusal? {
    if (scheme == null || !scheme.equals("content", ignoreCase = true)) {
      return Refusal.NOT_CONTENT
    }
    if (authority.isNullOrEmpty()) {
      return Refusal.NO_PROVIDER
    }
    if (ownAuthorities.any { it.equals(authority, ignoreCase = true) }) {
      return Refusal.OWN_FILE
    }
    return null
  }

  /** What the app tells the user about a file it did not open. */
  fun message(fileName: String, refusal: Refusal): String = when (refusal) {
    Refusal.OWN_FILE ->
      "\u201c$fileName\u201d is a file Borealis shared itself, so it was not opened again. " +
        "To add a pack file, use Import pack on the Homebrew page."
    Refusal.NOT_CONTENT, Refusal.NO_PROVIDER ->
      "\u201c$fileName\u201d was not opened: Borealis opens only files another app shares or opens with it."
    Refusal.TOO_LARGE -> "\u201c$fileName\u201d was not opened: it is larger than ${MAX_OPEN_BYTES / (1024 * 1024)} MB."
    Refusal.UNREADABLE -> "\u201c$fileName\u201d could not be read."
    Refusal.EMPTY -> "\u201c$fileName\u201d was not opened: it is empty."
  }

  /** The stream's bytes, or null when it holds more than [max] (never buffers more than [max]). */
  fun readCapped(input: InputStream, max: Int = MAX_OPEN_BYTES): ByteArray? {
    val out = ByteArrayOutputStream()
    val buf = ByteArray(CHUNK_BYTES)
    var total = 0L
    while (true) {
      val n = input.read(buf)
      if (n < 0) break
      total += n
      if (total > max) return null
      out.write(buf, 0, n)
    }
    return out.toByteArray()
  }
}
