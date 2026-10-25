# PACE scene format v1 (editor contract)

A UTF-8 JSON document describes a complete game. `format_version` is **1**;
unsupported versions are rejected before interpreting their contents. This is a
PACE data format, not a serialized Bevy world. The format is independent of Bevy
entities and renderer implementation. There is no scripting or asset loading in v1.

Use [scene-v1.schema.json](scene-v1.schema.json) for editor field completion and
shape validation. Generate it from the Rust types with
`cargo run --example scene_schema > docs/scene-v1.schema.json`. The schema checks
shape; `Scene::from_json` also checks the version, identifiers, references, bounds
and dialogue exits. Run the same validator before export:

```sh
cargo run --example validate_scene -- assets/last-light/game.json
```

Semantic errors are accumulated with JSON paths, for example:
`$.rooms[0].hotspots[1].outcome.room: unknown room 'lost'`.
Syntax/type errors include line/column and, when available, a field path. Unknown
fields, malformed enum tags and duplicate struct fields are errors, not ignored
extensions. A failed validation never produces a runnable scene.

## Document

Required fields: `format_version`, `game_id`, `title`, `start_room`, `rooms`,
`endings`. `items`, `flags`, `dialogues` default to empty arrays. There must be at
least one room and ending. The runtime supports any number of rooms; the fixture
has exactly three. IDs match `[a-z0-9_]+`, are case-sensitive, and must remain
stable across editor exports. IDs are unique within each collection: room,
item, flag, dialogue, ending; hotspots within a room; nodes within a dialogue;
choices within a node. The same string may occur in different collections.

- Item: `id`, `name`, `description`.
- Room: `id`, `name`, `description`, `color` (three sRGB bytes), `hotspots`.
- Ending: `id`, `text`.
- Flag declarations: strings; initially all false.
- Hotspot: `id`, `label`, `rect`, `blocked_text`, `text`, plus optional `once`
  (false), `requires` ([]), `effects` ([]), `outcome` (`{"type":"stay"}`).

`rect` is `[x, y, width, height]`, relative to the room viewport, with the origin
at its top-left. Coordinates must be finite, sizes positive, and the entire
rectangle within [0, 1]. This does not include the inventory/dialogue interface.
Overlapping hotspots use the **last declared visible hotspot** as the topmost.
Pointer hit tests include left/top edges and exclude right/bottom edges.
All player-facing text fields must be nonblank.

## Conditions, effects and outcomes

All conditions in `requires` must hold before an action can run:

| Shape | Meaning |
| --- | --- |
| `{"type":"has_item","item":"fuse"}` | Inventory contains item |
| `{"type":"flag","flag":"repaired","value":true}` | Flag equals value |

Effects execute in array order, atomically with the outcome after conditions pass:

| Shape | Meaning |
| --- | --- |
| `{"type":"give_item","item":"key"}` | Insert into inventory set (no duplicates) |
| `{"type":"remove_item","item":"fuse"}` | Remove if present (idempotent) |
| `{"type":"set_flag","flag":"repaired","value":true}` | Set flag; false clears it |

Use `has_item` when removal must require possession. There are no item counts or
item-combination recipes. Item use is a hotspot with a possession condition and
optional removal effect. Failed conditions display `blocked_text`, change no
state, and do not consume one-shot actions. A successful `once` hotspot disappears
and cannot be invoked again, even after returning to the room.

Exactly one outcome follows a hotspot's effects:

| Shape | Meaning |
| --- | --- |
| `{"type":"stay"}` | Stay in current room |
| `{"type":"room","room":"workshop"}` | Transition immediately to room |
| `{"type":"dialogue","dialogue":"keeper"}` | Open dialogue at its start node |
| `{"type":"end","ending":"safe_harbor"}` | Reach ending; disable further gameplay |

Every referenced item, flag, room, dialogue, node and ending must be declared.
The validator does not prove puzzle solvability or graph reachability; provide
state-transition tests and a complete walkthrough for each authored game.

## Dialogue

Dialogue: `id`, `start` (node ID), `nodes`. Node: `id`, `speaker`, `text`, `choices`.
Choice: `id`, `text`, optional `once` (false), `requires` ([]), `effects` ([]),
`next` (null). `next` references a node **in the same dialogue**; null/omitted closes
it. Every node needs an unconditional, repeatable null-next choice so players can
always leave, including after all one-shot choices are exhausted.

While a dialogue is active, room hotspots are disabled. Conditions filter the
available choices. Choice effects run once per successful selection; `once`
choices remain unavailable on later visits. Entering a node records it as visited;
selecting a choice records its dialogue/node/choice identity. Reopening a dialogue
starts at `start`, while loading a save resumes the active node. There are no
implicit node-entry effects. An ending is terminal until a new game or save load.

See [the hand-authored fixture](../assets/last-light/game.json) for all features.
Changing v1 semantics requires a new format version. Save files have a separate
version and a scene-content fingerprint; stable IDs alone do not make a changed
game compatible with an old save.
