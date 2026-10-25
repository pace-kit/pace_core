# Save format v1

`Game::save_json` / `load_json` handle UTF-8 JSON; `save_file` / `load_file` add
filesystem persistence. The desktop and terminal players share these APIs.

The envelope has required fields `save_version` (1), `game_id`,
`scene_fingerprint` and `state`. The fingerprint is lowercase SHA-256 of the typed
scene serialized by `serde_json::to_vec`: fixed struct field order, expanded
optional defaults, array order preserved. Source whitespace and object key order
do not matter. Any content change, including text, intentionally invalidates old
saves. No migrations are implemented. A future serializer/format change that
alters canonicalization must also change the save version.

`state` contains:

| Field | Representation |
| --- | --- |
| `room` | Current room ID |
| `inventory` | Sorted set of item IDs |
| `flags` | Sorted set of true flag IDs; absent means false |
| `used_hotspots` | Sorted set of `{room, hotspot}` for successful one-shot actions |
| `dialogue` | `{dialogue, node}` for the active conversation, or null |
| `visited_nodes` | Sorted set of `{dialogue, node}` |
| `chosen_choices` | Sorted set of `{dialogue, node, choice}` |
| `ending` | Ending ID, or null |

Loading checks version, game identity, fingerprint, all state references,
one-shot hotspot markers, dialogue/choice history consistency, and terminal
state consistency. The active dialogue/ending must be exposed by the current
room. Unknown fields are rejected. Loading is transactional: the running state
is replaced only after every check succeeds. Error messages include the save
path (file API) and offending JSON field. Saves are local state snapshots, not
signed proofs of play; validation does not prove that arbitrary edited states
are reachable or enforce every game-specific puzzle invariant.

Saving writes and syncs a temporary file beside the destination, then atomically
replaces the slot using `tempfile::persist`. Failed serialization, write, or
replacement leaves an existing slot intact. Parent directories must already
exist. Temp files are cleaned up on error. Concurrent writers use independent
temporary files; the last successful replacement wins. Directory metadata is not
synced, so power-loss durability depends on the platform/filesystem.

The desktop player uses `<game_id>.save.json` in its current directory by default;
its optional second positional argument chooses a different slot. F5/button
saves, F9/button loads, N/button starts a new game without deleting the slot.
Saving/loading is allowed during dialogue and after the ending. Missing,
corrupt, mismatched and inaccessible saves are shown as errors without resetting
the current game. The terminal fixture uses `last_light.save.json`.

The status line, hover state, window configuration and UI entities are transient;
loading rebuilds the interface from the restored state. Item selection is not
needed: inventory-gated hotspots use matching items automatically.
