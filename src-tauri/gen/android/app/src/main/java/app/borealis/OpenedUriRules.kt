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

  /**
   * Null when the URI may be read, otherwise why not. Only `content:` URIs of another app's provider
   * are read: Borealis opens a URI with its own permissions, so a `file:` URI (or a URI of one of
   * Borealis's own providers) would let another app make it read its private files.
   */
  fun refusal(scheme: String?, authority: String?, ownAuthorities: Set<String>): String? {
    if (scheme == null || !scheme.equals("content", ignoreCase = true)) {
      return "only content URIs are opened"
    }
    if (authority.isNullOrEmpty()) {
      return "the URI names no provider"
    }
    if (ownAuthorities.any { it.equals(authority, ignoreCase = true) }) {
      return "the URI points into Borealis itself"
    }
    return null
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
