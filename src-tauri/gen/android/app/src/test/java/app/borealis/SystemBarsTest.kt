package app.borealis

import android.content.res.Configuration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** B-48 sec. 5.9 and O9: the page's theme reaches the bar icons; the page learns the phone's dark mode from Android. */
class SystemBarsTest {
  @Test
  fun thePageThemeIsAppliedThroughThePostedRunnable() {
    val posted = mutableListOf<Runnable>()
    val applied = mutableListOf<Boolean>()
    val bridge = SystemBarsBridge({ posted.add(it) }, { applied.add(it) }, { false })

    bridge.setLight(true)
    bridge.setLight(false)
    assertEquals("nothing runs on the JavaBridge thread", emptyList<Boolean>(), applied)

    posted.forEach { it.run() }
    assertEquals(listOf(true, false), applied)
  }

  @Test
  fun thePageAsksAndroidForDarkMode() {
    var night = true
    val bridge = SystemBarsBridge({ it.run() }, { }, { night })
    assertTrue(bridge.isSystemNight())
    night = false
    assertFalse(bridge.isSystemNight())
  }

  @Test
  fun nightIsReadFromTheUiModeNightBits() {
    assertTrue(SystemTheme.isNight(Configuration.UI_MODE_NIGHT_YES or Configuration.UI_MODE_TYPE_NORMAL))
    assertFalse(SystemTheme.isNight(Configuration.UI_MODE_NIGHT_NO or Configuration.UI_MODE_TYPE_NORMAL))
    assertFalse(SystemTheme.isNight(Configuration.UI_MODE_NIGHT_UNDEFINED))
  }

  @Test
  fun theChangeScriptOnlyFiresThePageEvent() {
    assertEquals("window.dispatchEvent(new CustomEvent('borealis-system-theme'))", SystemTheme.CHANGED_SCRIPT)
  }
}
