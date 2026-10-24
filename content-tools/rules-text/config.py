"""Per-pack settings: source PDF, attribution text, and which headings are out of scope."""
from dataclasses import dataclass
from typing import Dict, List, Tuple


@dataclass(frozen=True)
class PackConfig:
    pack_id: str
    name: str
    description: str
    ruleset: str  # dnd2014 | dnd2024
    pdf_name: str
    document: str  # "SRD 5.1"
    version_label: str  # "5.1"
    attribution: str  # the SRD's required statement, verbatim in meaning and wording
    modification: str
    exclude: Tuple[str, ...]  # regexes over "A > B > C" heading paths: sections dropped (element chapters)
    scope_notes: Tuple[str, ...]
    limits: Tuple[str, ...]
    indent_paragraphs: bool  # first lines are indented in this typesetting
    first_page: int = 1  # pages before this are front matter (legal text, contents) and are skipped
    outline_root_depth: int = 0
    # Chapter headings the typesetting leaves out: {first H2 heading under the missing chapter: chapter title}
    virtual_chapters: Dict[str, str] = None
    # Chapters whose H4 headings are a flat list of entries (the SRD 5.2.1 glossary)
    flat_chapters: Tuple[str, ...] = ()


ATTRIBUTION_51 = (
    'This work includes material taken from the System Reference Document 5.1 ("SRD 5.1") by Wizards of the Coast '
    "LLC and available at https://dnd.wizards.com/resources/systems-reference-document. The SRD 5.1 is licensed "
    "under the Creative Commons Attribution 4.0 International License available at "
    "https://creativecommons.org/licenses/by/4.0/legalcode."
)
ATTRIBUTION_521 = (
    'This work includes material from the System Reference Document 5.2.1 ("SRD 5.2.1") by Wizards of the Coast '
    "LLC, available at https://www.dndbeyond.com/srd. The SRD 5.2.1 is licensed under the Creative Commons "
    "Attribution 4.0 International License, available at https://creativecommons.org/licenses/by/4.0/legalcode."
)
MODIFICATION = (
    "Modified: the SRD text was extracted from the PDF text layer by a deterministic script, split into one section "
    "per heading, and converted to HTML fragments (paragraphs, lists, tables); line breaks and hyphenation were "
    "normalised, tables were flattened to rows of cells, and the chapters that Borealis models as typed content "
    "elements (see COVERAGE.md) were left out. No material outside the SRD {label} is included."
)

PACKS: List[PackConfig] = [
    PackConfig(
        pack_id="srd-5.1-rules-text",
        name="SRD 5.1 Rules Text",
        description="The rules chapters of the SRD 5.1 as searchable text sections, one per heading, with heading path and page.",
        ruleset="dnd2014",
        pdf_name="SRD_CC_v5.1.pdf",
        document="SRD 5.1",
        version_label="5.1",
        attribution=ATTRIBUTION_51,
        modification=MODIFICATION.format(label="5.1"),
        exclude=(
            r"^(Races|Barbarian|Bard|Cleric|Druid|Fighter|Monk|Paladin|Ranger|Rogue|Sorcerer|Warlock|Wizard|Feats)( > |$)",
            r"^Beyond 1st Level > Backgrounds( > |$)",
            r"^Spellcasting > (Spell Lists|Spell Descriptions)( > |$)",
            r"^Magic Items > Magic Items A-Z( > |$)",
            r"^Monsters > Monsters \(",
            r"^Appendix MM-",
        ),
        scope_notes=(
            "Front matter before the first chapter: the legal page and the attached licence text (in ATTRIBUTION.md).",
            "Races, the twelve class chapters (Barbarian to Wizard) and Feats (typed species, class and feat elements).",
            "Beyond 1st Level > Backgrounds (typed background element).",
            "Spellcasting > Spell Lists and Spell Descriptions (typed spell elements).",
            "Magic Items > Magic Items A-Z (typed item elements).",
            "Monsters (A) to Monsters (Z), Appendix MM-A Miscellaneous Creatures and MM-B Nonplayer Characters (typed monster packs).",
        ),
        limits=(
            "The PDF prints no chapter heading for Adventuring, Combat and Gamemastering; those three H1 parents are "
            "added to the heading path (`virtual_chapters` in `config.py`) so the sections group as in the SRD.",
            "Tables are flattened to rows of cells in reading order; a table cell that wraps over several lines becomes several rows.",
            "The Conditions appendix is kept although the conditions also exist as elements; deduplicate when indexing.",
        ),
        indent_paragraphs=True,
        first_page=3,
        virtual_chapters={
            "Time": "Adventuring",
            "The Order of Combat": "Combat",
            "Traps": "Gamemastering",
        },
    ),
    PackConfig(
        pack_id="srd-5.2.1-rules-text",
        name="SRD 5.2.1 Rules Text",
        description="The rules chapters of the SRD 5.2.1 as searchable text sections, one per heading (including every Rules Glossary entry), with heading path and page.",
        ruleset="dnd2024",
        pdf_name="SRD_CC_v5.2.1.pdf",
        document="SRD 5.2.1",
        version_label="5.2.1",
        attribution=ATTRIBUTION_521,
        modification=MODIFICATION.format(label="5.2.1"),
        exclude=(
            r"^Classes( > |$)",
            r"^Character Origins > Character Species > ",
            r"^Feats > Feat Descriptions > ",
            r"^Spells > Spell Descriptions > ",
            "^Magic Items > Magic Items A[–-]Z > ",
            "^Monsters A[–-]Z( > |$)",
            r"^Animals( > |$)",
        ),
        scope_notes=(
            "Front matter before the first chapter: the legal page and the table of contents (the licence text is in ATTRIBUTION.md).",
            "Classes (typed class and subclass elements).",
            "Character Origins > Character Species entries and Feats > Feat Descriptions entries (typed species and feat elements; the chapter and section texts stay).",
            "Spells > Spell Descriptions entries (typed spell elements; Gaining Spells and Casting Spells stay).",
            "Magic Items > Magic Items A-Z entries (typed item elements; the Magic Items rules sections stay).",
            "Monsters A-Z and Animals (typed monster pack).",
        ),
        limits=(
            "Tables are flattened to rows of cells in reading order; a table cell that wraps over several lines becomes several rows.",
            "The Rules Glossary is split into one section per entry (H4 headings in a flat list under the chapter). "
            "Glossary entries overlap the typed Condition elements; deduplicate when indexing.",
            "Sidebar titles are printed in small capitals and are decoded from the glyph sizes; they appear as bold paragraphs, not headings.",
        ),
        indent_paragraphs=False,
        first_page=5,
        outline_root_depth=1,
        virtual_chapters={},
        flat_chapters=("Rules Glossary",),
    ),
]
