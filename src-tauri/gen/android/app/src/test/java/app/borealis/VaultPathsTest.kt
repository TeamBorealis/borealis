package app.borealis

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** B-221: the tree-relative path rules `VaultPlugin` walks the document tree with. */
class VaultPathsTest {
  @Test
  fun pathsSplitIntoFoldersAndAName() {
    assertEquals(listOf("Places", "Harbor Town.md"), VaultPaths.parts("Places/Harbor Town.md"))
    assertEquals(listOf("Places"), VaultPaths.folders("Places/Harbor Town.md"))
    assertEquals("Harbor Town.md", VaultPaths.name("Places/Harbor Town.md"))
    assertEquals("Places", VaultPaths.parent("Places/Harbor Town.md"))
    assertEquals("", VaultPaths.parent("Home.md"))
    assertEquals(emptyList<String>(), VaultPaths.folders("Home.md"))
    assertEquals(emptyList<String>(), VaultPaths.parts(""))
  }

  @Test
  fun joinPutsNamesInFolders() {
    assertEquals("Home.md", VaultPaths.join("", "Home.md"))
    assertEquals("A/B/c.md", VaultPaths.join("A/B", "c.md"))
  }

  @Test
  fun hiddenNamesAndTheSpareCopyAreHidden() {
    assertTrue(VaultPaths.isHidden(".obsidian"))
    assertTrue(VaultPaths.isHidden(".trash"))
    assertFalse(VaultPaths.isHidden("Notes"))
    val spare = VaultPaths.spareName("1234")
    assertEquals(".borealis-1234.tmp", spare)
    assertTrue(VaultPaths.isHidden(spare))
  }
}
