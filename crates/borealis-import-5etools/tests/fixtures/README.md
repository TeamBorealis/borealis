# Test fixtures

`data/` mimics the layout of a 5etools `data/` folder, but holds only entries that
5etools flags as part of the Systems Reference Document (`srd` for 5.1, `srd52` for 5.2.1;
for magic item variants the flag sits inside `inherits`). The `Basic Rules` flags are not
used. The spell source lookup keeps the `class` lists of the chosen spells only.

The importer never ships or commits 5etools data. These files are a small,
programmatically filtered subset (a dozen spells, two classes with their SRD features,
a few races, items, a background, two feats) used to test the converters.
`srd_fixtures.rs` fails if an unflagged entry is ever added here.

## Attribution

This work includes material taken from the System Reference Document 5.1 ("SRD 5.1") and
the System Reference Document 5.2.1 ("SRD 5.2.1") by Wizards of the Coast LLC, available at
https://dnd.wizards.com/resources/systems-reference-document. The SRD 5.1 and 5.2.1 are
licensed under the Creative Commons Attribution 4.0 International License, available at
https://creativecommons.org/licenses/by/4.0/legalcode.

The text was taken through the 5etools data format; changes: filtered to the entries
named above, otherwise unmodified.
