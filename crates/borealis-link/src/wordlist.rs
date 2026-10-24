//! The 256-word list behind typed codes (`42-ember-lantern`).
//!
//! Written for Borealis and licensed with it (MIT OR Apache-2.0): plain, common English nouns
//! that are easy to read aloud and type on a phone, 3 to 8 lowercase letters, no two alike. No
//! third-party list is used (the EFF lists are CC-BY 3.0 and the PGP list's terms are unclear),
//! so there is nothing to attribute. Changing the list breaks codes between app versions, so
//! it is append-never: a new list needs a new code format.

/// Exactly 256 words: one word is one byte of the code's secret.
pub const WORDS: [&str; 256] = [
    "amber", "anchor", "anvil", "apple", "arrow", "ash", "aspen", "autumn", //
    "badger", "banner", "barley", "basin", "beacon", "bear", "beetle", "bell", //
    "birch", "bison", "blade", "bloom", "boar", "bolt", "bonfire", "boulder", //
    "bow", "bramble", "brass", "breeze", "brick", "bridge", "brook", "bucket", //
    "cabin", "candle", "canyon", "carrot", "castle", "cedar", "chalk", "cherry", //
    "cinder", "citadel", "clay", "cliff", "clover", "cobalt", "comet", "copper", //
    "coral", "cotton", "cove", "crane", "creek", "crow", "crown", "crystal", //
    "cypress", "dagger", "daisy", "dawn", "deer", "desert", "dew", "dove", //
    "dragon", "drum", "dune", "eagle", "earth", "echo", "elder", "elm", //
    "ember", "falcon", "feather", "fern", "ferry", "field", "fig", "finch", //
    "flame", "flint", "forest", "forge", "fox", "frost", "galaxy", "garden", //
    "garnet", "gate", "glacier", "glade", "goblet", "gold", "goose", "granite", //
    "grape", "gravel", "griffin", "grove", "gull", "hammer", "harbor", "harp", //
    "hawk", "hazel", "heather", "hedge", "helm", "heron", "hill", "hollow", //
    "honey", "horizon", "horn", "island", "ivory", "ivy", "jade", "jasper", //
    "juniper", "kettle", "kestrel", "key", "kite", "lake", "lantern", "larch", //
    "lark", "laurel", "lava", "leaf", "lemon", "lily", "lion", "lotus", //
    "lute", "lynx", "maple", "marble", "marsh", "meadow", "mesa", "meteor", //
    "mint", "mirror", "mist", "moon", "moss", "moth", "mountain", "nectar", //
    "nest", "nettle", "north", "oak", "oasis", "ocean", "olive", "onyx", //
    "opal", "orchard", "otter", "owl", "panther", "path", "pearl", "pebble", //
    "pepper", "pine", "plum", "pond", "poplar", "prism", "quarry", "quartz", //
    "quill", "rabbit", "rain", "raven", "reed", "ridge", "river", "robin", //
    "rose", "ruby", "rune", "saddle", "saffron", "sage", "salmon", "sand", //
    "sapphire", "scroll", "shadow", "shell", "shield", "silver", "slate", "snow", //
    "sparrow", "spear", "spice", "spruce", "star", "stone", "storm", "stream", //
    "summit", "sun", "swan", "sword", "thistle", "thorn", "thunder", "tide", //
    "tiger", "timber", "topaz", "torch", "tower", "trail", "trout", "tulip", //
    "tundra", "valley", "velvet", "violet", "wagon", "walnut", "wand", "wave", //
    "whale", "wheat", "willow", "wind", "winter", "wolf", "wren", "yarrow", //
    "acorn", "bay", "cloud", "delta", "fable", "glow", "jewel", "knot", //
    "linen", "marigold", "nova", "orbit", "petal", "ripple", "satin", "zephyr", //
];

/// Index of a word (exact, lowercase).
pub fn index_of(word: &str) -> Option<u8> {
    WORDS
        .iter()
        .position(|w| *w == word)
        .and_then(|i| u8::try_from(i).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn words_are_unique_short_lowercase_letters() {
        let set: BTreeSet<&str> = WORDS.iter().copied().collect();
        assert_eq!(set.len(), 256, "duplicate word");
        for w in WORDS {
            assert!((3..=8).contains(&w.len()), "{w}");
            assert!(w.bytes().all(|b| b.is_ascii_lowercase()), "{w}");
        }
        assert_eq!(
            index_of("ember").map(|i| WORDS[usize::from(i)]),
            Some("ember")
        );
        assert_eq!(index_of("Ember"), None);
    }
}
