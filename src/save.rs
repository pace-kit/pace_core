//! Versioned saves, validated before replacing live state; atomic file replacement.
use crate::{runtime::*, scene::*};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, io::Write, path::Path};

pub const SAVE_VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Save {
    save_version: u32,
    game_id: String,
    scene_fingerprint: String,
    state: GameState,
}

#[derive(Debug)]
pub struct SaveError(pub String);
impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SaveError {}

fn fingerprint(scene: &Scene) -> String {
    // Typed serialization fixes field order and expands defaults. Whitespace and
    // object-key order in the source file do not invalidate an otherwise identical scene.
    let bytes = serde_json::to_vec(scene).expect("validated scene is serializable");
    format!("{:x}", Sha256::digest(bytes))
}

impl Game {
    pub fn save_json(&self) -> Result<String, SaveError> {
        serde_json::to_string_pretty(&Save {
            save_version: SAVE_VERSION,
            game_id: self.scene().game_id.clone(),
            scene_fingerprint: fingerprint(self.scene()),
            state: self.state.clone(),
        })
        .map_err(|e| SaveError(format!("could not serialize save: {e}")))
    }

    /// Failure leaves the live game untouched, including an active conversation.
    pub fn load_json(&mut self, json: &str) -> Result<(), SaveError> {
        let header: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| SaveError(format!("$: invalid save JSON: {e}")))?;
        if let Some(version) = header.get("save_version").and_then(|v| v.as_u64()) {
            if version != u64::from(SAVE_VERSION) {
                return Err(SaveError(format!(
                    "$.save_version: unsupported save version {version}; expected {SAVE_VERSION}"
                )));
            }
        }
        let mut de = serde_json::Deserializer::from_str(json);
        let save: Save = serde_path_to_error::deserialize(&mut de)
            .map_err(|e| SaveError(format!("$.{}: {}", e.path(), e.inner())))?;
        if save.game_id != self.scene().game_id {
            return Err(SaveError(format!(
                "$.game_id: save belongs to '{}', expected '{}'",
                save.game_id,
                self.scene().game_id
            )));
        }
        if save.scene_fingerprint != fingerprint(self.scene()) {
            return Err(SaveError("$.scene_fingerprint: scene content has changed; this save is incompatible (no v1 migration)".into()));
        }
        validate_state(self.scene(), &save.state)?;
        self.state = save.state;
        Ok(())
    }

    /// Writes/syncs a temporary file in the destination directory, then atomically
    /// replaces the slot. Serialization/write/rename failures preserve the old file.
    pub fn save_file(&self, path: impl AsRef<Path>) -> Result<(), SaveError> {
        let path = path.as_ref();
        let json = self.save_json()?;
        let io_error = |e: std::io::Error| SaveError(format!("{}: {e}", path.display()));
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
        temp.write_all(json.as_bytes()).map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        temp.persist(path).map_err(|e| io_error(e.error))?;
        Ok(())
    }

    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<(), SaveError> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path)
            .map_err(|e| SaveError(format!("{}: {e}", path.display())))?;
        self.load_json(&json)
            .map_err(|e| SaveError(format!("{}: {e}", path.display())))
    }
}

fn validate_state(scene: &Scene, state: &GameState) -> Result<(), SaveError> {
    let mut errors = Vec::new();
    let mut check = |valid: bool, path: &str, message: String| {
        if !valid {
            errors.push(Diagnostic {
                path: format!("$.state.{path}"),
                message,
            });
        }
    };
    check(
        scene.rooms.iter().any(|r| r.id == state.room),
        "room",
        format!("unknown room '{}'", state.room),
    );
    for (i, item) in state.inventory.iter().enumerate() {
        check(
            scene.items.iter().any(|x| &x.id == item),
            &format!("inventory[{i}]"),
            format!("unknown item '{item}'"),
        );
    }
    for (i, flag) in state.flags.iter().enumerate() {
        check(
            scene.flags.contains(flag),
            &format!("flags[{i}]"),
            format!("unknown flag '{flag}'"),
        );
    }
    for (i, h) in state.used_hotspots.iter().enumerate() {
        let hotspot = scene
            .rooms
            .iter()
            .find(|r| r.id == h.room)
            .and_then(|r| r.hotspots.iter().find(|x| x.id == h.hotspot));
        check(
            hotspot.is_some_and(|h| h.once),
            &format!("used_hotspots[{i}]"),
            format!(
                "expected existing one-shot hotspot '{}/{}'",
                h.room, h.hotspot
            ),
        );
    }
    let node = |p: &DialoguePosition| {
        scene
            .dialogues
            .iter()
            .find(|d| d.id == p.dialogue)
            .and_then(|d| d.nodes.iter().find(|n| n.id == p.node))
    };
    for (i, p) in state.visited_nodes.iter().enumerate() {
        check(
            node(p).is_some(),
            &format!("visited_nodes[{i}]"),
            format!("unknown dialogue/node '{}/{}'", p.dialogue, p.node),
        );
    }
    for (i, c) in state.chosen_choices.iter().enumerate() {
        let p = DialoguePosition {
            dialogue: c.dialogue.clone(),
            node: c.node.clone(),
        };
        check(
            node(&p).is_some_and(|n| n.choices.iter().any(|x| x.id == c.choice)),
            &format!("chosen_choices[{i}]"),
            format!("unknown choice '{}/{}/{}'", c.dialogue, c.node, c.choice),
        );
        check(
            state.visited_nodes.contains(&p),
            &format!("chosen_choices[{i}]"),
            "chosen choice's node must have been visited".into(),
        );
    }
    if let Some(p) = &state.dialogue {
        check(
            node(p).is_some(),
            "dialogue",
            format!("unknown dialogue/node '{}/{}'", p.dialogue, p.node),
        );
        check(
            state.visited_nodes.contains(p),
            "dialogue",
            "active node must have been visited".into(),
        );
        check(
            state.ending.is_none(),
            "dialogue",
            "an ended game cannot have active dialogue".into(),
        );
        check(scene.rooms.iter().find(|r| r.id == state.room).is_some_and(|r| r.hotspots.iter().any(|h| matches!(&h.outcome, Outcome::Dialogue { dialogue } if dialogue == &p.dialogue))), "dialogue", "active dialogue must belong to a hotspot in the current room".into());
    }
    if let Some(ending) = &state.ending {
        check(
            scene.endings.iter().any(|e| &e.id == ending),
            "ending",
            format!("unknown ending '{ending}'"),
        );
        check(
            scene
                .rooms
                .iter()
                .find(|r| r.id == state.room)
                .is_some_and(|r| {
                    r.hotspots
                        .iter()
                        .any(|h| matches!(&h.outcome, Outcome::End { ending: e } if e == ending))
                }),
            "ending",
            "ending must belong to a hotspot in the current room".into(),
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(SaveError(Diagnostics(errors).to_string()))
    }
}
