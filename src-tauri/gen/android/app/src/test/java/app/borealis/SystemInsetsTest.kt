package app.borealis

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** HW1 A2: the page learns the status bar, navigation bar and cutout sizes in CSS px (and, B-48 WP8, the keyboard). */
class SystemInsetsTest {
  @Test
  fun devicePixelsBecomeCssPixels() {
    assertEquals("{\"t\":41,\"r\":0,\"b\":14,\"l\":0,\"k\":0}", SystemInsets.json(144, 0, 48, 0, 3.5f))
    assertEquals("{\"t\":24,\"r\":41,\"b\":0,\"l\":0,\"k\":0}", SystemInsets.json(84, 144, 0, 0, 3.5f))
  }

  @Test
  fun theKeyboardIsReportedInCssPixels() {
    assertEquals("{\"t\":24,\"r\":0,\"b\":24,\"l\":0,\"k\":312}", SystemInsets.json(63, 0, 63, 0, 2.625f, 819))
  }

  @Test
  fun negativeOrMissingDensityIsSafe() {
    assertEquals("{\"t\":0,\"r\":0,\"b\":0,\"l\":10,\"k\":0}", SystemInsets.json(-5, 0, 0, 10, 0f, -40))
  }

  @Test
  fun theScriptOnlyCallsThePageHook() {
    val script = SystemInsets.applyScript("{\"t\":1,\"r\":2,\"b\":3,\"l\":4}")
    assertTrue(script.startsWith("window.__borealisApplyInsets&&window.__borealisApplyInsets("))
    assertTrue(script.contains("{\"t\":1,\"r\":2,\"b\":3,\"l\":4}"))
  }

  @Test
  fun theBridgeAnswersTheLatestInsets() {
    SystemInsets.update(SystemInsets.json(70, 0, 0, 0, 3.5f))
    assertEquals("{\"t\":20,\"r\":0,\"b\":0,\"l\":0,\"k\":0}", InsetsBridge().get())
  }
}
