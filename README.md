# PACE Core

A minimal data-driven point-and-click adventure runtime. **The Last Light** is an
original, hand-authored three-room fixture with pickups, inventory-gated actions,
branching dialogue, room transitions, versioned save/load and a reachable ending.

## Play

Requires Rust **1.97.1+**. This project pins **Bevy 0.20.0** (stable) and commits
`Cargo.lock`. See [Bevy's learning resources](https://bevy.org/learn/) for the engine
API and platform setup. The old Bevy 0.6 scaffold has been updated.

```sh
cargo run --locked --features desktop --example last_light
# Optional path to another v1 scene:
cargo run --locked --features desktop --example last_light -- path/to/game.json path/to/slot.json
```

Click the labeled rectangles, or press their numbers (1–9). Items are used
automatically by matching hotspots. Dialogue is modal. **N** starts a new game.
**F5** saves; **F9** loads (also available as buttons).
The desktop player uses native Bevy UI; on Linux it needs X11 and a Vulkan-capable
driver (Mesa software rendering also works). Audio, gamepads and 3D are disabled.

For a terminal player or a complete self-checking demonstration (no GPU):

```sh
cargo run --locked --example console
cargo run --locked --example console -- --walkthrough
```

Follow the [playable walkthrough](docs/walkthrough.md). The automated run saves
and restores an active conversation, a repaired lantern and the ending, using an
isolated temporary slot. Normal play saves to `last_light.save.json` in the
current directory. Failed loads preserve the running game.

## Editor integration

- [Versioned scene contract](docs/scene-format.md)
- [Save format and compatibility](docs/save-format.md)
- [Generated JSON Schema](docs/scene-v1.schema.json)
- [Fixture source](assets/last-light/game.json) and [asset license](assets/last-light/LICENSE.md)

```sh
cargo run --locked --example validate_scene -- assets/last-light/game.json
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Core tests use Bevy's headless `MinimalPlugins`; the default feature set does not
compile a renderer or need a display. `Game::apply` is the deterministic transition
boundary. Its state is read-only to callers; errors never partially mutate it.
`AdventurePlugin` exposes ordered `AdventureCommand` and `AdventureResult`
messages. Insert a validated `Game` resource, send commands before `AdventureStep`,
and consume results after it. The desktop viewer uses this same plugin.

The implementation intentionally omits navigation/pathfinding, animation,
scripting, audio, item counts, localization and editor UI. PACE's original empty
`build` and `debug` modules remain available.
