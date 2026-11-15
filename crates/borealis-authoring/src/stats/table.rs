//! The curated statistics: names outside the generated families (abilities, saves, skills, speeds, spellcasting,
//! levels, companion), each with its label and group. Every entry is used by a bundled pack: the test
//! `curated_entries_are_used_by_bundled_content` keeps the table from going stale.

/// `(name, label, group id)`.
pub(super) const CURATED: &[(&str, &str, &str)] = &[
    // Armor Class
    ("ac:calculation", "Armor Class (calculated)", "armorClass"),
    ("ac:misc", "Armor Class bonus", "armorClass"),
    ("ac:shield", "Armor Class from a shield", "armorClass"),
    ("ac:dexterity", "Armor Class from Dexterity", "armorClass"),
    ("ac:armored", "Armor Class in armor", "armorClass"),
    (
        "ac:armored:armor",
        "Armor Class in armor, the armor's base",
        "armorClass",
    ),
    (
        "ac:armored:dexterity",
        "Armor Class in armor, Dexterity",
        "armorClass",
    ),
    (
        "ac:armored:dexterity:cap",
        "Armor Class in armor, Dexterity limit",
        "armorClass",
    ),
    (
        "ac:armored:enhancement",
        "Armor Class in armor, magic bonus",
        "armorClass",
    ),
    (
        "ac:armored:misc",
        "Armor Class in armor, other bonus",
        "armorClass",
    ),
    ("ac:unarmored", "Armor Class without armor", "armorClass"),
    (
        "ac:unarmored:base",
        "Armor Class without armor, base",
        "armorClass",
    ),
    (
        "ac:unarmored:dexterity",
        "Armor Class without armor, Dexterity",
        "armorClass",
    ),
    (
        "ac:unarmored defense barbarian",
        "Unarmored Defense (Barbarian)",
        "armorClass",
    ),
    (
        "ac:unarmored defense barbarian:base",
        "Unarmored Defense (Barbarian), base",
        "armorClass",
    ),
    (
        "ac:unarmored defense barbarian:constitution",
        "Unarmored Defense (Barbarian), Constitution",
        "armorClass",
    ),
    (
        "ac:unarmored defense barbarian:dexterity",
        "Unarmored Defense (Barbarian), Dexterity",
        "armorClass",
    ),
    (
        "ac:unarmored defense monk",
        "Unarmored Defense (Monk)",
        "armorClass",
    ),
    (
        "ac:unarmored defense monk:base",
        "Unarmored Defense (Monk), base",
        "armorClass",
    ),
    (
        "ac:unarmored defense monk:dexterity",
        "Unarmored Defense (Monk), Dexterity",
        "armorClass",
    ),
    (
        "ac:unarmored defense monk:wisdom",
        "Unarmored Defense (Monk), Wisdom",
        "armorClass",
    ),
    (
        "ac:draconic resilience",
        "Draconic Resilience Armor Class",
        "armorClass",
    ),
    (
        "ac:draconic resilience:base",
        "Draconic Resilience, base",
        "armorClass",
    ),
    (
        "ac:draconic resilience:charisma",
        "Draconic Resilience, Charisma",
        "armorClass",
    ),
    (
        "ac:draconic resilience:dexterity",
        "Draconic Resilience, Dexterity",
        "armorClass",
    ),
    // Hit points
    ("hp", "Hit point maximum", "hitPoints"),
    (
        "dark ones blessing:temp hp",
        "Dark One's Blessing temporary hit points",
        "hitPoints",
    ),
    ("lay on hands:hp pool", "Lay On Hands pool", "hitPoints"),
    // Senses
    ("darkvision:range", "Darkvision range", "senses"),
    (
        "superior darkvision:range",
        "Superior Darkvision range",
        "senses",
    ),
    ("blindsight:range", "Blindsight range", "senses"),
    ("truesight:range", "Truesight range", "senses"),
    // Attacks
    ("attacks:extra", "Extra attacks", "attacks"),
    ("extra attack:count", "Extra Attack, attacks", "attacks"),
    ("melee:damage", "Melee damage bonus", "attacks"),
    ("ranged:attack", "Ranged attack bonus", "attacks"),
    (
        "damage:strength melee",
        "Damage bonus, Strength melee attacks",
        "attacks",
    ),
    ("rage damage", "Rage damage", "attacks"),
    ("barbarian rage:damage", "Rage damage (2014)", "attacks"),
    (
        "martial arts:attack",
        "Martial Arts attack bonus",
        "attacks",
    ),
    (
        "martial arts:damage",
        "Martial Arts damage bonus",
        "attacks",
    ),
    ("martial arts:dice", "Martial Arts die", "attacks"),
    ("sneak-attack:count", "Sneak Attack dice", "attacks"),
    ("sneak-attack:die", "Sneak Attack die", "attacks"),
    (
        "agonizing blast:damage",
        "Agonizing Blast damage",
        "attacks",
    ),
    ("lifedrinker:damage", "Lifedrinker damage", "attacks"),
    ("breath-weapon:dc", "Breath Weapon save DC", "attacks"),
    ("breath-weapon:dice count", "Breath Weapon dice", "attacks"),
    ("breath-weapon:dice size", "Breath Weapon die", "attacks"),
    (
        "draconic ancestry damage type",
        "Draconic Ancestry damage type",
        "attacks",
    ),
    ("draconic-ancestor", "Draconic Ancestor", "attacks"),
    (
        "draconic-ancestor:damage type",
        "Draconic Ancestor damage type",
        "attacks",
    ),
    ("draconic-ancestry", "Draconic Ancestry", "attacks"),
    (
        "draconic-ancestry:breath",
        "Draconic Ancestry breath weapon",
        "attacks",
    ),
    (
        "draconic-ancestry:damage type",
        "Draconic Ancestry damage type (2014)",
        "attacks",
    ),
    // Level and proficiency (added to the generated entries)
    ("initiative", "Initiative bonus", "levels"),
    ("initiative:misc", "Initiative, other bonus", "levels"),
    // Class counters
    ("barbarian rage:count", "Rages", "classCounters"),
    (
        "bardic inspiration:count",
        "Bardic Inspiration uses",
        "classCounters",
    ),
    (
        "bardic inspiration:die",
        "Bardic Inspiration die",
        "classCounters",
    ),
    (
        "bardic inspiration die",
        "Bardic Inspiration die (2024)",
        "classCounters",
    ),
    ("song of rest:die", "Song of Rest die", "classCounters"),
    (
        "channel divinity:count",
        "Channel Divinity uses",
        "classCounters",
    ),
    ("divine sense:count", "Divine Sense uses", "classCounters"),
    (
        "cleansing touch:count",
        "Cleansing Touch uses",
        "classCounters",
    ),
    (
        "aura of protection:range",
        "Aura of Protection range",
        "classCounters",
    ),
    (
        "aura of protection:save bonus",
        "Aura of Protection bonus",
        "classCounters",
    ),
    (
        "aura of courage:range",
        "Aura of Courage range",
        "classCounters",
    ),
    (
        "aura of devotion:range",
        "Aura of Devotion range",
        "classCounters",
    ),
    ("favored enemy:count", "Favored Enemy uses", "classCounters"),
    (
        "natural explorer:count",
        "Natural Explorer terrains",
        "classCounters",
    ),
    ("indomitable:usage", "Indomitable uses", "classCounters"),
    ("ki:points", "Ki points", "classCounters"),
    ("ki:dc", "Ki save DC", "classCounters"),
    (
        "monk:unarmored movement",
        "Unarmored Movement bonus",
        "classCounters",
    ),
    ("monk:slow fall", "Slow Fall reduction", "classCounters"),
    (
        "monk:deflect missiles bonus",
        "Deflect Missiles reduction",
        "classCounters",
    ),
    (
        "monk:wholeness of body",
        "Wholeness of Body healing",
        "classCounters",
    ),
    ("sorcerer:sorcery points", "Sorcery Points", "classCounters"),
    ("focus save dc", "Focus save DC", "classCounters"),
    (
        "intimidating presence:dc",
        "Intimidating Presence save DC",
        "classCounters",
    ),
    ("tranquility:dc", "Tranquility save DC", "classCounters"),
    // Other
    ("attunement:max", "Items you can attune to", "other"),
    (
        "thieves tools:proficiency",
        "Thieves' Tools proficiency",
        "other",
    ),
];
