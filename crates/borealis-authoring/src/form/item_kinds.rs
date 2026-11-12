//! The item forms (spec 4.9): Item, Weapon, Armor, Magic Item, Item Pack and Equipment Package. Each field is bound
//! to every model location that holds it ([`super::item_codec`]); the GENERAL / RULES / DESCRIPTION parts every form
//! shares come from [`super::kinds`].

use super::codec::Codec;
use super::item_codec::ItemField;
use super::kinds::{
    field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, GENERAL, ONLY_2024, TABS,
};
use borealis_content::Ruleset;

const ITEM: &str = "Item";
const COST: &str = "Cost and weight";
const INVENTORY: &str = "In the inventory";
const WEAPON: &str = "Weapon";
const ARMOR: &str = "Armor";
const MAGIC: &str = "Magic";
const CONTENTS: &str = "Contents";
const PACKAGE: &str = "Starting equipment";
const DETAILS: &str = "Details";

const CATEGORIES: &[(&str, &str)] = &[
    ("Adventuring Gear", "Adventuring Gear"),
    ("Ammunition", "Ammunition"),
    ("Armor", "Armor"),
    ("Equipment Packs", "Equipment Packs"),
    ("Food, Drink, and Lodging", "Food, Drink, and Lodging"),
    ("Gaming Sets", "Gaming Sets"),
    ("Magic Armor", "Magic Armor"),
    ("Magic Weapons", "Magic Weapons"),
    ("Mounts and Other Animals", "Mounts and Other Animals"),
    ("Musical Instruments", "Musical Instruments"),
    ("Potions", "Potions"),
    ("Rings", "Rings"),
    ("Rods", "Rods"),
    ("Spellcasting Focus", "Spellcasting Focus"),
    ("Staffs", "Staffs"),
    ("Tack and Harness", "Tack and Harness"),
    ("Tools", "Tools"),
    ("Trade Goods", "Trade Goods"),
    ("Wands", "Wands"),
    ("Weapons", "Weapons"),
    ("Wondrous Items", "Wondrous Items"),
];

const COINS: &[(&str, &str)] = &[
    ("", "-"),
    ("cp", "Copper (cp)"),
    ("sp", "Silver (sp)"),
    ("ep", "Electrum (ep)"),
    ("gp", "Gold (gp)"),
    ("pp", "Platinum (pp)"),
];

const SLOTS: &[(&str, &str)] = &[
    ("", "Not worn or held"),
    ("onehand", "One hand"),
    ("twohand", "Two hands"),
    ("onehand,secondary", "One hand or the off hand"),
    ("secondary", "Off hand"),
    ("body", "Body"),
    ("head", "Head"),
    ("neck", "Neck"),
    ("shoulders", "Shoulders"),
    ("arms", "Arms"),
    ("hands", "Hands"),
    ("ring", "Ring finger"),
    ("waist", "Waist"),
    ("feet", "Feet"),
    ("misc", "Somewhere else"),
];

const GEAR_TYPES: &[(&str, &str)] = &[
    ("Tool", "Tool"),
    ("Ammunition", "Ammunition"),
    ("Gaming Set", "Gaming Set"),
    ("Musical Instrument", "Musical Instrument"),
    ("Spellcasting Focus", "Spellcasting Focus"),
    ("Mount", "Mount"),
    ("Vehicle", "Vehicle"),
];

const MAGIC_TYPES: &[(&str, &str)] = &[
    ("Wondrous item", "Wondrous item"),
    ("Weapon", "Weapon"),
    ("Armor", "Armor"),
    ("Ring", "Ring"),
    ("Potion", "Potion"),
    ("Wand", "Wand"),
    ("Staff", "Staff"),
    ("Rod", "Rod"),
    ("Scroll", "Scroll"),
];

const RARITIES: &[(&str, &str)] = &[
    ("Common", "Common"),
    ("Uncommon", "Uncommon"),
    ("Rare", "Rare"),
    ("Very Rare", "Very Rare"),
    ("Legendary", "Legendary"),
    ("Artifact", "Artifact"),
    ("Varies", "Varies"),
];

const ARMOR_TYPES: &[(&str, &str)] = &[
    ("", "-"),
    ("Light", "Light armor"),
    ("Medium", "Medium armor"),
    ("Heavy", "Heavy armor"),
    ("Shield", "Shield"),
];

const fn item(f: ItemField) -> Codec {
    Codec::Item(f)
}

/// Fields every gear form lists first: category, cost, currency, weight.
fn category() -> FieldDef {
    field(
        "category",
        "Category",
        GENERAL,
        Control::Combo,
        Codec::SetterText("category"),
    )
    .choices(CATEGORIES, true)
    .section(ITEM)
    .help("The group it is listed under in the equipment list.")
}

fn cost_and_weight() -> Vec<FieldDef> {
    vec![
        field(
            "cost",
            "Cost",
            GENERAL,
            Control::Text,
            item(ItemField::Cost),
        )
        .section(COST),
        field(
            "currency",
            "Coin",
            GENERAL,
            Control::Combo,
            item(ItemField::Currency),
        )
        .choices(COINS, false)
        .section(COST)
        .help("Gold pieces unless you pick another coin."),
        field(
            "weight",
            "Weight (lb.)",
            GENERAL,
            Control::Text,
            item(ItemField::Weight),
        )
        .section(COST)
        .help("In pounds, like 2 or 0.5, or a word such as Varies."),
    ]
}

fn slot() -> FieldDef {
    field(
        "slot",
        "Worn or held on",
        GENERAL,
        Control::Combo,
        Codec::SetterText("slot"),
    )
    .choices(SLOTS, false)
    .section(INVENTORY)
    .help("Only items worn or held can be equipped.")
}

fn proficiency(section: &'static str) -> FieldDef {
    field(
        "proficiency",
        "Proficiency",
        GENERAL,
        Control::Reference,
        item(ItemField::Proficiency),
    )
    .picker(&["Proficiency"])
    .section(section)
    .help("The proficiency a character needs to use it well.")
}

fn contents() -> FieldDef {
    field(
        "contents",
        "Items in the pack",
        GENERAL,
        Control::Contents,
        item(ItemField::Contents),
    )
    .section(CONTENTS)
    .help("Opening the pack in the inventory gives these items.")
}

/// The flags of an item in the inventory (spec 4.9 "stackable, consumable, container, hidden").
fn inventory_flags() -> Vec<FieldDef> {
    vec![
        field(
            "stackable",
            "Stacks with others",
            GENERAL,
            Control::Check,
            Codec::SetterFlag("stackable"),
        )
        .help("Several of it share one inventory row with a count (arrows, rations).")
        .section(INVENTORY),
        field(
            "consumable",
            "Used up when used",
            GENERAL,
            Control::Check,
            Codec::SetterFlag("consumable"),
        )
        .help("Using it uses one up (a potion, a scroll).")
        .section(INVENTORY),
        field(
            "container",
            "Holds other items",
            GENERAL,
            Control::Check,
            Codec::SetterFlag("stash"),
        )
        .help("Other items can be put inside it (a backpack, a pouch).")
        .section(INVENTORY),
        field(
            "capacity",
            "Holds up to (lb.)",
            GENERAL,
            Control::Text,
            item(ItemField::SetterAttr {
                setter: "stash",
                attr: "lb",
                mirror: None,
                needs: "Tick \u{201c}Holds other items\u{201d} first, or clear this field.",
            }),
        )
        .section(INVENTORY),
        field(
            "hidden",
            "Hidden in the inventory",
            GENERAL,
            Control::Check,
            Codec::SetterFlag("inventory-hidden"),
        )
        .section(INVENTORY)
        .expert(),
    ]
}

/// The shared parts around the kind's own fields, the GENERAL ones under "Details" (after the item's sections);
/// prerequisites and "more than once" are for experts on items.
fn with_shared(own: Vec<FieldDef>) -> Vec<FieldDef> {
    let rarely = ["requirements", "prerequisiteText", "allowDuplicate"];
    let shared = tail().into_iter().map(|f| {
        let f = if f.tab == GENERAL {
            f.section(DETAILS)
        } else {
            f
        };
        if rarely.contains(&f.key) {
            f.expert()
        } else {
            f
        }
    });
    head().into_iter().chain(own).chain(shared).collect()
}

fn kind(
    id: &'static str,
    label: &'static str,
    plural: &'static str,
    description: &'static str,
    ty: &'static [&'static str],
    editions: &'static [Ruleset],
    own: Vec<FieldDef>,
) -> KindDef {
    KindDef {
        id,
        label,
        plural,
        description,
        group: KindGroup::Main,
        element_types: ty,
        parent_kinds: &[],
        editions,
        tabs: TABS,
        fields: with_shared(own),
    }
}

fn gear() -> KindDef {
    let mut own = vec![
        category(),
        field(
            "itemType",
            "Item type",
            GENERAL,
            Control::Combo,
            Codec::SetterText("type"),
        )
        .choices(GEAR_TYPES, true)
        .section(ITEM),
        proficiency(ITEM),
    ];
    own.extend(cost_and_weight());
    own.push(slot());
    own.extend(inventory_flags());
    own.push(contents());
    kind(
        "item",
        "Item",
        "Items",
        "Gear, tools, ammunition, mounts and other things to carry.",
        &["Item"],
        BOTH,
        own,
    )
}

fn weapon() -> KindDef {
    let mut own = vec![
        category(),
        field(
            "weaponCategory",
            "Weapon category",
            GENERAL,
            Control::Reference,
            item(ItemField::WeaponCategory),
        )
        .picker(&["Weapon Category"])
        .section(WEAPON)
        .help("Simple or martial, melee or ranged. Monks can use simple melee weapons, and martial melee weapons with the Light property, with Martial Arts."),
        field(
            "damage",
            "Damage",
            GENERAL,
            Control::Text,
            item(ItemField::Damage),
        )
        .section(WEAPON)
        .help("Dice, like 1d8."),
        field(
            "damageType",
            "Damage type",
            GENERAL,
            Control::Reference,
            item(ItemField::DamageType),
        )
        .picker(&["Damage Type"])
        .section(WEAPON),
        field(
            "versatile",
            "Two-handed damage",
            GENERAL,
            Control::Text,
            item(ItemField::Versatile),
        )
        .section(WEAPON)
        .help("For a versatile weapon: the dice when used with two hands."),
        field("range", "Range (ft.)", GENERAL, Control::Text, item(ItemField::Range))
            .section(WEAPON)
            .help("Normal and long range, like 20/60."),
        field(
            "properties",
            "Properties",
            GENERAL,
            Control::References,
            item(ItemField::WeaponProperties),
        )
        .help("Its weapon properties, such as Finesse or Light.")
        .picker(&["Weapon Property"])
        .section(WEAPON),
        field(
            "mastery",
            "Mastery",
            GENERAL,
            Control::References,
            item(ItemField::Mastery),
        )
        .picker(&["Mastery Property"])
        .section(WEAPON)
        .help("The mastery property a character with Weapon Mastery can use (2024)."),
        field(
            "weaponGroups",
            "Weapon groups",
            GENERAL,
            Control::References,
            item(ItemField::WeaponGroups),
        )
        .picker(&["Weapon Group"])
        .section(WEAPON)
        .expert(),
        proficiency(WEAPON),
    ];
    own.extend(cost_and_weight());
    // a weapon is held: a new one starts in one hand, so a character can wield it (the engine equips only items
    // worn or held somewhere)
    own.push(slot().default("\"onehand\""));
    kind(
        "weapon",
        "Weapon",
        "Weapons",
        "A weapon with damage, properties and mastery.",
        &["Weapon"],
        BOTH,
        own,
    )
}

fn armor() -> KindDef {
    let mut own = vec![
        category(),
        field(
            "armorType",
            "Armor type",
            GENERAL,
            Control::Combo,
            item(ItemField::ArmorType),
        )
        .choices(ARMOR_TYPES, false)
        .section(ARMOR)
        .help("Light armor adds the whole Dexterity modifier, medium armor up to 2, heavy armor none."),
        field("baseAc", "Armor Class", GENERAL, Control::Number, item(ItemField::BaseAc))
            .range(0, 30)
            .section(ARMOR)
            .help("The base Armor Class, or what a shield adds."),
        field(
            "armorClassText",
            "Armor Class as shown",
            GENERAL,
            Control::Text,
            Codec::SetterText("armorClass"),
        )
        .section(ARMOR)
        .help("How the equipment list shows it, like 12 + Dex (max 2)."),
        field(
            "strength",
            "Strength needed",
            GENERAL,
            Control::Number,
            item(ItemField::Strength),
        )
        .help("The Strength score needed to wear it without losing 10 feet of speed; 0 for none.")
        .range(0, 30)
        .section(ARMOR),
        field(
            "stealth",
            "Disadvantage on Stealth checks",
            GENERAL,
            Control::Check,
            item(ItemField::Stealth),
        )
        .section(ARMOR),
        proficiency(ARMOR),
    ];
    own.extend(cost_and_weight());
    // armor is worn: a new one starts on the body, so a character can put it on (the engine equips only items worn or
    // held somewhere); a shield's maker moves it to the off hand
    own.push(
        slot()
            .default("\"body\"")
            .help("Armor is worn on the body, a shield held in the off hand. Only items worn or held can be equipped."),
    );
    kind(
        "armor",
        "Armor",
        "Armor",
        "Armor or a shield, with its Armor Class.",
        &["Armor"],
        BOTH,
        own,
    )
}

fn magic_item() -> KindDef {
    let mut own = vec![
        category(),
        field(
            "magicType",
            "Item type",
            GENERAL,
            Control::Combo,
            item(ItemField::MagicType),
        )
        .choices(MAGIC_TYPES, true)
        .section(MAGIC),
        field(
            "typeDetail",
            "Item type detail",
            GENERAL,
            Control::Text,
            item(ItemField::SetterAttr {
                setter: "type",
                attr: "addition",
                mirror: Some("item_subtype"),
                needs: "Fill in the item type first, or clear this field.",
            }),
        )
        .section(MAGIC)
        .help("Shown after the type, like \u{201c}any sword\u{201d}."),
        field(
            "rarity",
            "Rarity",
            GENERAL,
            Control::Combo,
            item(ItemField::Rarity),
        )
        .help("Common to artifact; shown with the item and in the Compendium.")
        .choices(RARITIES, true)
        .section(MAGIC),
        field(
            "attunement",
            "Requires attunement",
            GENERAL,
            Control::Check,
            item(ItemField::Attunement),
        )
        .help("Its magic works only for a character attuned to it (at most three items).")
        .section(MAGIC),
        field(
            "attunementBy",
            "Attunement by",
            GENERAL,
            Control::Text,
            item(ItemField::SetterAttr {
                setter: "attunement",
                attr: "addition",
                mirror: Some("attunement_by"),
                needs: "Tick \u{201c}Requires attunement\u{201d} first, or clear this field.",
            }),
        )
        .section(MAGIC)
        .help("Who can attune to it, like \u{201c}by a wizard\u{201d}."),
        field(
            "appliesToWeapon",
            "Made from a weapon",
            GENERAL,
            Control::TagFilter,
            item(ItemField::AppliesTo("weapon")),
        )
        .section(MAGIC)
        .help("The weapons it can be made from; the character picks one when adding it."),
        field(
            "appliesToArmor",
            "Made from armor",
            GENERAL,
            Control::TagFilter,
            item(ItemField::AppliesTo("armor")),
        )
        .section(MAGIC)
        .help("The armor it can be made from; the character picks one when adding it."),
        field(
            "enhancement",
            "Bonus (+N)",
            GENERAL,
            Control::Number,
            item(ItemField::Enhancement),
        )
        .range(0, 5)
        .section(MAGIC)
        .help("Added to attack and damage rolls with a weapon, or to the Armor Class of armor."),
        field(
            "nameFormat",
            "Name when made",
            GENERAL,
            Control::Text,
            Codec::SetterText("name-format"),
        )
        .section(MAGIC)
        .help("Write {{parent}} for the base item's name and {{enhancement}} for the bonus, like {{parent}} +{{enhancement}}."),
    ];
    own.extend(cost_and_weight());
    own.push(slot());
    kind(
        "magicItem",
        "Magic item",
        "Magic items",
        "A magic item, with rarity, attunement and its effects as rules.",
        &["Magic Item"],
        BOTH,
        own,
    )
}

fn item_pack() -> KindDef {
    let mut own = vec![category()];
    own.extend(cost_and_weight());
    own.push(contents());
    kind(
        "itemPack",
        "Item pack",
        "Item packs",
        "A pack of items that opens into its contents.",
        &["Item Pack"],
        BOTH,
        own,
    )
}

fn equipment_package() -> KindDef {
    let own = vec![
        field(
            "owner",
            "Belongs to",
            GENERAL,
            Control::Reference,
            item(ItemField::PackageOwner),
        )
        .picker(&["Class", "Background"])
        .section(PACKAGE)
        .help("The class or background that offers it."),
        field(
            "options",
            "Options",
            GENERAL,
            Control::PackageOptions,
            item(ItemField::PackageOptions),
        )
        .section(PACKAGE)
        .help("The player takes one option: its items and coins."),
    ];
    KindDef {
        // edited inside its class too (the class's EQUIPMENT tab, WB-16)
        parent_kinds: &["class", "background"],
        ..kind(
            "equipmentPackage",
            "Starting equipment",
            "Starting equipment",
            "The starting equipment choices of a class or background (A or B).",
            &["Equipment Package"],
            ONLY_2024,
            own,
        )
    }
}

/// The six item forms, in New element order.
pub(crate) fn item_kinds() -> Vec<KindDef> {
    vec![
        gear(),
        weapon(),
        armor(),
        magic_item(),
        item_pack(),
        equipment_package(),
    ]
}
