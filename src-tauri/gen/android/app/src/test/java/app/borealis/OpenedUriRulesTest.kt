package app.borealis

import java.io.ByteArrayInputStream
import java.io.InputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Security review Z: what MainActivity agrees to read when a file is opened with Borealis. */
class OpenedUriRulesTest {
  private val own = setOf("app.borealis.fileprovider", "app.borealis.androidx-startup")

  @Test
  fun anotherAppsContentUriIsRead() {
    assertNull(OpenedUriRules.refusal("content", "com.android.providers.downloads.documents", own))
    assertNull(OpenedUriRules.refusal("CONTENT", "com.google.android.apps.docs.storage", own))
  }

  @Test
  fun fileUrisAreRefused() {
    assertNotNull(OpenedUriRules.refusal("file", "", own))
    assertNotNull(OpenedUriRules.refusal("file", null, own))
    assertNotNull(OpenedUriRules.refusal(null, "x", own))
    assertNotNull(OpenedUriRules.refusal("https", "example.com", own))
  }

  @Test
  fun borealisOwnProvidersAreRefused() {
    assertNotNull(OpenedUriRules.refusal("content", "app.borealis.fileprovider", own))
    assertNotNull(OpenedUriRules.refusal("content", "APP.BOREALIS.FILEPROVIDER", own))
    assertNotNull(OpenedUriRules.refusal("content", null, own))
  }

  @Test
  fun readsUpToTheCap() {
    val bytes = ByteArray(1000) { it.toByte() }
    val got = OpenedUriRules.readCapped(ByteArrayInputStream(bytes), 1000)
    assertNotNull(got)
    assertTrue(bytes.contentEquals(got!!))
  }

  @Test
  fun anOversizedOrEndlessStreamIsRefusedWithoutBufferingIt() {
    assertNull(OpenedUriRules.readCapped(ByteArrayInputStream(ByteArray(1001)), 1000))
    var served = 0L
    val endless = object : InputStream() {
      override fun read(): Int = 0
      override fun read(b: ByteArray, off: Int, len: Int): Int {
        served += len
        return len
      }
    }
    assertNull(OpenedUriRules.readCapped(endless, 1 shl 20))
    assertTrue("stopped right after the cap, read $served", served <= (1 shl 20) + 64 * 1024)
  }

  @Test
  fun theCapIsTheBundleFileCap() {
    assertEquals(64 * 1024 * 1024, OpenedUriRules.MAX_OPEN_BYTES)
  }
}
