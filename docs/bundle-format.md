# borealis-bundle/1

The zip one of the user's devices hands to another of their own devices (Send to device / Receive from
device, device-sync WP A1, backlog B-14). Implemented by `crates/borealis-bundle`. Packs travel only between
the user's own devices (SY2); this is not a sharing format. No network, no UI in the crate.

## Layout

```
manifest.json                 required; the table of contents and integrity record
characters/<key>.json         one native character file each (key = file stem, e.g. c_<uuid>)
packs/<pack>/<rel path>       a user content pack folder from <data>/library/<pack>/
settings.portable.json        optional; the portable subset of settings.json
campaigns/                    RESERVED (EM3 not built). Readers refuse a bundle that uses it.
```

Deflate-compressed zip, no encryption, no symlinks, no directory entries needed. Every file entry must be
listed in the manifest and every listed file must exist (no stray or missing entries).

## manifest.json

```json
{
  "format": "borealis-bundle/1",
  "app_version": "1.0.0",
  "created_ms": 1700000000000,
  "source_device": "Alon's PC",
  "items": [
    {
      "kind": "character",
      "key": "c_...",
      "sha256": "<item digest>",
      "size": 31234,
      "edited_at_ms": 1700000000000,
      "files": [{ "path": "characters/c_....json", "sha256": "...", "size": 31234 }],
      "required_packs": [{ "id": "my-pack", "bundled": false }, { "id": "srd-5.1", "bundled": true }]
    },
    { "kind": "pack", "key": "my-pack", "sha256": "...", "size": 9000, "edited_at_ms": 1700000000000,
      "files": [{ "path": "packs/my-pack/pack.json", "sha256": "...", "size": 120 }] }
  ],
  "settings": { "path": "settings.portable.json", "sha256": "...", "size": 200 },
  "campaigns": []
}
```

* Items are sorted by (kind, key). `kind` is `character` or `pack`.
* **Item digest** (`sha256`): for a single-file item, the file's SHA-256; otherwise SHA-256 over
  `"<path>\0<file sha256>\n"` for each file in path order. A device computes the same digest from its own
  files (`scan_characters`, `scan_packs`), so equal content means equal digest.
* **`edited_at_ms`**: the character format (P1, `format_version` 2) has no edit timestamp, so the file's
  modification time is used. For a pack, the newest modification time among its files.
* **`required_packs`** (characters only): every pack that defines an element id which appears as a string
  anywhere in the character JSON. `bundled: true` packs ship with every install and are never in the bundle;
  `bundled: false` are user packs, included when the sender selected them (`PackSelection::Required` by
  default for a character send). The app seeds bundled packs through `PackIndex::add_pack(id, true, ids)`.
* `campaigns` is the reserved slot: must be empty now.
* Unknown format: `borealis-bundle/<other>` is refused with "unsupported format"; anything else is "not a
  bundle".

## What never travels (enforced in code, writer and reader)

`is_excluded(path)` refuses any path with a segment `search`, `models`, `import-drafts`, `ai-profile`, `sync`,
`logs`, `extracted`; any segment starting `ai-` or `.tmp-`; the files `ai-host-cert.der`, `ai-host-key.der`,
`ai-host.json`, `ai-setup.json`, `settings.json`, `content-prefs.json`, `state.json`; and extensions `.der .pem
.key .p12 .pfx .pk8 .keystore`. The directory builder skips such files inside pack folders (reported in
`BuildReport.skipped_pack_files`); `BundleWriter` and `Bundle::open` refuse them outright.
Bundled packs and 5etools source folders are never included (only `<data>/library/<pack>/` folders that have
a `pack.json`). Settings are an allow-list (`PORTABLE_SETTINGS_KEYS`): `playerName`, `defaultRuleset`,
`abilityMethod`, `pointBuyBudget`, `abilityMinBase`, `abilityMaxBase`, `abilityCapEnabled`,
`confirmRemoveLevel`, `confirmEquipWeaponNotInAttacks`, `sheet`, `accentColor`. Anything else (`ai.*`,
`documentsDir`, `secondaryContentDir`, `mobile`, density/tile/row size, panel states) is dropped when writing
and refused when reading.

## Reader limits (`Limits::default()`)

Entries 20,000; items 10,000; one file 64 MiB; all files 512 MiB; manifest 8 MiB; expansion ratio 200x for
entries over 1 MiB; pack path depth 8. Entry names must be plain relative `/` paths (no `..`, `.`, empty
segments, absolute paths, drive letters, backslashes, NUL). Symlinks and encrypted entries are refused.
Declared sizes are never trusted: bodies are read through a `take(declared + 1)` and checked against both
size and SHA-256. `Bundle::open` reads only the directory and manifest; `read_item`/`read_settings`/`verify_all`
read and verify bodies. Character files must be JSON objects; deeper validation (`borealis-character`,
`drop_unsafe_portrait`, pack validator) is the applier's job before writing.

## Apply-on-receive rule (SY4, latest edit wins)

`plan_receive(manifest, local_items, installed_pack_ids)` returns one decision per incoming item:

| Situation | Action |
|---|---|
| not on this device | `New` (write) |
| same digest | `Identical` (no-op, whatever the times) |
| different digest, incoming `edited_at_ms` larger | `ReplaceWithIncoming` (old local copy to `<data>/sync/trash/`, kept 30 days, then write) |
| different digest, local larger | `KeepLocalNewer` (keep local, report) |
| different digest, equal times | `KeepLocalTie` (keep local, report: neither side is newer) |

No keep-both copies. After writing, the applier should set the file's modification time to the item's
`edited_at_ms` so the next comparison finds equal stamps. Items present only locally are untouched (a
bundle never deletes). `missing_packs` per entry lists required packs that are neither in the bundle nor
installed. Comparison is wall-clock across devices; clock skew can mis-order edits made within the skew
(accepted for manual sync; the trash copy keeps the loser).

## API sketch

```rust
let (out, manifest, report) = build_bundle(file, &BuildRequest { .. })?;   // from the on-disk layout
let mut b = Bundle::open(reader)?;                                         // structural validation
let plan = plan_receive(b.manifest(), &scan_local, &installed_pack_ids);
let data = b.read_item(ItemKind::Character, key)?;                         // verified bytes
```
