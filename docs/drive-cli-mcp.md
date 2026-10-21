# Driving Borealis from another program (CLI and MCP write mode)

Borealis can be driven end to end without its window: create characters, make every builder
choice, level up, manage equipment and spells, import content and export the sheet. There are
two front ends on **one shared tool layer** (`src-tauri/src/drive/`):

- the **CLI**: `borealis <command> --library <dir> [args]`, one command per process;
- the **MCP write mode**: `borealis --mcp --allow-write --library <dir>`, a stdio MCP server for
  an AI assistant (Claude Code, Codex, any MCP client).

Command names and arguments are identical in both. Every action goes through the same functions
the app's UI calls, so the rules engine accepts or refuses it exactly as it does in the window.
Nothing in this layer uses AI: it is deterministic. Desktop only (macOS, Windows, Linux).

The executable is the app itself:

| Platform | Executable |
|---|---|
| macOS | `/Applications/Borealis.app/Contents/MacOS/borealis` |
| Windows | `C:\Program Files\Borealis\borealis.exe` (or where you installed it) |
| Linux | `borealis` (package) or the AppImage path |

Below, `borealis` stands for that path.

## The library folder

`--library <dir>` is required for both front ends. It is a Borealis data folder with the app's
own layout (`characters/`, imported content packs under `library/`, settings), created when
missing. Use a dedicated folder for automation. Pointing it at the app's own data folder drives
your real characters; do not do that while the app is open, as both would write the same files.

Changes are saved after every successful edit (like the app's autosave). A refused edit changes
nothing. `delete_character` moves the file to `<library>/trash/`; only `permanent: true` deletes
it for good.

## CLI

    borealis help                     # all commands
    borealis help pick                # one command's arguments
    borealis <command> --library <dir> [--<arg> <value>]... [--json | --human]

- Arguments are the MCP argument names; `-` and `_` are interchangeable (`--slot-id`,
  `--slot_id`). Booleans may be given alone (`--feats`). Lists are comma-separated
  (`--sources PHB,DMG`). `--scores str=15,dex=14` or a JSON object.
- `--args '<json>'` passes all arguments as one JSON object, exactly as the MCP tool takes them.
- Output is JSON on stdout by default; `--human` prints a short text form.
- `--content-dir <dir>` overrides the bundled content folder (default: next to the executable).
- Exit codes: `0` done; `1` the action was refused or failed (stdout carries
  `{"error": {"code", "message"}}`, stderr the message); `2` usage error (unknown command or
  argument, missing `--library`).

Example session (2024 rules, a level-3 cleric):

    L=~/borealis-drive
    ID=$(borealis create-character --name Mira --ruleset 2024 --library $L | jq -r .id)
    borealis list-slots --id $ID --open-only --library $L --human
    borealis list-options --id $ID --slot-id <slot> --query Human --library $L --human
    borealis pick --id $ID --slot-id <slot> --element-id ID_2024_SPECIES_HUMAN --library $L
    borealis pick --id $ID --slot-id <class slot> --element-id ID_2024_CLASS_CLERIC --library $L
    borealis set-abilities --id $ID --scores str=10,dex=12,con=14,int=8,wis=15,cha=13 --library $L
    borealis level-up --id $ID --library $L
    borealis level-up --id $ID --library $L
    borealis search-items --id $ID --query Mace --library $L --human
    borealis add-item --id $ID --item-id ID_2024_WEAPON_MACE --library $L
    borealis equip-item --id $ID --uid <uid from the inventory> --library $L
    borealis get-character --id $ID --section spells --library $L
    borealis prepare-spell --id $ID --spell-id <spell id> --library $L
    borealis export-sheet --id $ID --path sheets/mira.pdf --library $L

## Commands (CLI) = tools (MCP)

| Group | Tools |
|---|---|
| Library | `list_characters`, `create_character`, `open_character`, `get_character` (sections: summary, abilities, stats, slots, spells, inventory, full), `save_character` (`as_name` saves a copy), `close_character`, `delete_character` |
| Builder | `list_slots`, `list_options`, `pick`, `unpick`, `level_up`, `add_multiclass`, `set_abilities` |
| Gear and spells | `search_items`, `add_item`, `remove_item`, `equip_item`, `unequip_item`, `attune_item`, `prepare_spell`, `unprepare_spell`, `export_sheet` |
| Content | `list_packs`, `import_index` (HTTPS index URL), `scan_5etools`, `import_5etools` (a 5etools `data` folder or `.zip`), `import_content_file` (XML, JSON, `.index` or `.zip`) |

Multiclassing: `add_multiclass` returns the new slot (`addedSlots`); `pick` a class into it,
then `level_up` with its `class_slot_id`.

## MCP

Without `--allow-write`, `borealis --mcp` is the read-only content server (7 tools: content
search, element lookup, validation, class sweep, encounter maths, rules terms). With
`--allow-write --library <dir>` the character tools above are added after them; `tools/list`
reflects the mode. Both flags are needed together. Same protocol as the read-only server
(MCP 2026-07-28 with the `initialize` handshake for older clients).

Claude Code:

    claude mcp add borealis -- /Applications/Borealis.app/Contents/MacOS/borealis \
      --mcp --allow-write --library ~/borealis-drive

Codex (`~/.codex/config.toml`):

    [mcp_servers.borealis]
    command = "/Applications/Borealis.app/Contents/MacOS/borealis"
    args = ["--mcp", "--allow-write", "--library", "/Users/me/borealis-drive"]

or `codex mcp add borealis -- <executable> --mcp --allow-write --library <dir>`.

A refused action comes back as a tool result with `isError: true` and the engine's reason, e.g.
an option the slot does not offer or a spell over the preparation limit.

## Security model

- **Opt-in writes.** The default MCP server cannot change anything. Write tools exist only with
  `--allow-write`, and only together with an explicit `--library` folder.
- **One folder.** Characters, imported packs and settings live in the library folder. Every path
  argument (sheet output, 5etools folder or zip, content file) must resolve inside it: relative
  paths are relative to it, `..` and symlinks leading out are refused (`outside_library`). A sheet
  is written only to a file ending in `.pdf` or `.html`, so it cannot overwrite a character or a
  settings file.
- **No network** except `import_index`, which downloads the given HTTPS index and its files.
- **No destructive deletes by default**: deleted characters go to `<library>/trash/` unless
  `permanent: true`.
- **No AI inside.** Every number is computed by the engine; imports are rules-based.
- Logs go to stderr; stdout carries only results (CLI) or protocol messages (MCP).

Known limit: the Windows release build is a GUI-subsystem executable, so CLI output is visible
when redirected or piped (`borealis help > out.txt`, `| more`), not printed straight into a
console window. MCP over pipes is unaffected.
