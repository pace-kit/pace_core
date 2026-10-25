//! Deterministic state transitions shared by the headless and desktop players.
use crate::scene::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HotspotRef {
    pub room: String,
    pub hotspot: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DialoguePosition {
    pub dialogue: String,
    pub node: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceRef {
    pub dialogue: String,
    pub node: String,
    pub choice: String,
}

/// Durable state uses authored IDs only; no Bevy Entity IDs or asset handles.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameState {
    pub room: String,
    pub inventory: BTreeSet<String>,
    pub flags: BTreeSet<String>,
    pub used_hotspots: BTreeSet<HotspotRef>,
    pub dialogue: Option<DialoguePosition>,
    pub visited_nodes: BTreeSet<DialoguePosition>,
    pub chosen_choices: BTreeSet<ChoiceRef>,
    pub ending: Option<String>,
}

#[derive(Clone, Debug, Message)]
pub enum AdventureCommand {
    Interact { hotspot: String },
    Choose { choice: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionError {
    Finished,
    DialogueActive,
    NoDialogue,
    UnknownHotspot(String),
    UnknownChoice(String),
    Unavailable(String),
}
impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Finished => write!(f, "The story has ended. Start a new game or load a save."),
            Self::DialogueActive => {
                write!(f, "Finish the conversation before using a room hotspot.")
            }
            Self::NoDialogue => write!(f, "There is no active conversation."),
            Self::UnknownHotspot(id) => write!(f, "No hotspot '{id}' in the current room."),
            Self::UnknownChoice(id) => write!(f, "No choice '{id}' at the current dialogue node."),
            Self::Unavailable(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for ActionError {}

#[derive(Resource, Clone, Debug)]
pub struct Game {
    pub(crate) scene: ValidatedScene,
    pub(crate) state: GameState,
}

impl Game {
    pub fn new(scene: ValidatedScene) -> Self {
        let state = GameState {
            room: scene.scene().start_room.clone(),
            inventory: BTreeSet::new(),
            flags: BTreeSet::new(),
            used_hotspots: BTreeSet::new(),
            dialogue: None,
            visited_nodes: BTreeSet::new(),
            chosen_choices: BTreeSet::new(),
            ending: None,
        };
        Self { scene, state }
    }
    pub fn scene(&self) -> &Scene {
        self.scene.scene()
    }
    pub fn state(&self) -> &GameState {
        &self.state
    }
    pub fn room(&self) -> &Room {
        self.scene()
            .rooms
            .iter()
            .find(|r| r.id == self.state.room)
            .expect("validated room")
    }
    pub fn active_node(&self) -> Option<&DialogueNode> {
        let p = self.state.dialogue.as_ref()?;
        self.scene()
            .dialogues
            .iter()
            .find(|d| d.id == p.dialogue)?
            .nodes
            .iter()
            .find(|n| n.id == p.node)
    }
    pub fn meets(&self, conditions: &[Condition]) -> bool {
        conditions.iter().all(|c| match c {
            Condition::HasItem { item } => self.state.inventory.contains(item),
            Condition::Flag { flag, value } => self.state.flags.contains(flag) == *value,
        })
    }
    pub fn visible_hotspots(&self) -> Vec<&Hotspot> {
        if self.state.dialogue.is_some() || self.state.ending.is_some() {
            return vec![];
        }
        self.room()
            .hotspots
            .iter()
            .filter(|h| {
                !h.once
                    || !self.state.used_hotspots.contains(&HotspotRef {
                        room: self.state.room.clone(),
                        hotspot: h.id.clone(),
                    })
            })
            .collect()
    }
    pub fn available_choices(&self) -> Vec<&Choice> {
        let Some(p) = &self.state.dialogue else {
            return vec![];
        };
        self.active_node()
            .into_iter()
            .flat_map(|n| &n.choices)
            .filter(|c| {
                self.meets(&c.requires)
                    && (!c.once
                        || !self.state.chosen_choices.contains(&ChoiceRef {
                            dialogue: p.dialogue.clone(),
                            node: p.node.clone(),
                            choice: c.id.clone(),
                        }))
            })
            .collect()
    }
    /// Hit test normalized room coordinates. The last authored rectangle is on top.
    pub fn hotspot_at(&self, x: f32, y: f32) -> Option<&Hotspot> {
        self.visible_hotspots().into_iter().rev().find(|h| {
            let [left, top, w, height] = h.rect;
            x >= left && x < left + w && y >= top && y < top + height
        })
    }
    pub fn apply(&mut self, command: AdventureCommand) -> Result<String, ActionError> {
        if self.state.ending.is_some() {
            return Err(ActionError::Finished);
        }
        match command {
            AdventureCommand::Interact { hotspot } => self.interact(&hotspot),
            AdventureCommand::Choose { choice } => self.choose(&choice),
        }
    }
    fn interact(&mut self, id: &str) -> Result<String, ActionError> {
        if self.state.dialogue.is_some() {
            return Err(ActionError::DialogueActive);
        }
        let h = self
            .room()
            .hotspots
            .iter()
            .find(|h| h.id == id)
            .cloned()
            .ok_or_else(|| ActionError::UnknownHotspot(id.into()))?;
        let key = HotspotRef {
            room: self.state.room.clone(),
            hotspot: h.id.clone(),
        };
        if (h.once && self.state.used_hotspots.contains(&key)) || !self.meets(&h.requires) {
            return Err(ActionError::Unavailable(h.blocked_text));
        }
        self.effects(&h.effects);
        if h.once {
            self.state.used_hotspots.insert(key);
        }
        match h.outcome {
            Outcome::Stay {} => {}
            Outcome::Room { room } => self.state.room = room,
            Outcome::Dialogue { dialogue } => {
                let d = self
                    .scene()
                    .dialogues
                    .iter()
                    .find(|d| d.id == dialogue)
                    .expect("validated dialogue");
                self.enter_node(DialoguePosition {
                    dialogue: d.id.clone(),
                    node: d.start.clone(),
                });
            }
            Outcome::End { ending } => self.state.ending = Some(ending),
        }
        Ok(h.text)
    }
    fn choose(&mut self, id: &str) -> Result<String, ActionError> {
        let p = self.state.dialogue.clone().ok_or(ActionError::NoDialogue)?;
        let c = self
            .active_node()
            .expect("validated dialogue node")
            .choices
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| ActionError::UnknownChoice(id.into()))?;
        let key = ChoiceRef {
            dialogue: p.dialogue.clone(),
            node: p.node,
            choice: c.id.clone(),
        };
        if (c.once && self.state.chosen_choices.contains(&key)) || !self.meets(&c.requires) {
            return Err(ActionError::Unavailable(format!(
                "Choice '{id}' is unavailable."
            )));
        }
        self.effects(&c.effects);
        self.state.chosen_choices.insert(key);
        if let Some(node) = c.next {
            self.enter_node(DialoguePosition {
                dialogue: p.dialogue,
                node,
            });
        } else {
            self.state.dialogue = None;
        }
        Ok(c.text)
    }
    fn enter_node(&mut self, p: DialoguePosition) {
        self.state.visited_nodes.insert(p.clone());
        self.state.dialogue = Some(p);
    }
    fn effects(&mut self, effects: &[Effect]) {
        for effect in effects {
            match effect {
                Effect::GiveItem { item } => {
                    self.state.inventory.insert(item.clone());
                }
                Effect::RemoveItem { item } => {
                    self.state.inventory.remove(item);
                }
                Effect::SetFlag { flag, value } => {
                    if *value {
                        self.state.flags.insert(flag.clone());
                    } else {
                        self.state.flags.remove(flag);
                    }
                }
            }
        }
    }
}

/// Command results remain separate from durable game state (including rejected input).
#[derive(Message, Debug)]
pub struct AdventureResult(pub Result<String, ActionError>);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdventureStep;

/// Insert a `Game` resource before running. Send commands before `AdventureStep`
/// and consume results after it. Commands are applied in message order.
pub struct AdventurePlugin;
impl Plugin for AdventurePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AdventureCommand>()
            .add_message::<AdventureResult>()
            .add_systems(Update, process_commands.in_set(AdventureStep));
    }
}
fn process_commands(
    mut game: ResMut<Game>,
    mut commands: MessageReader<AdventureCommand>,
    mut results: MessageWriter<AdventureResult>,
) {
    for command in commands.read() {
        results.write(AdventureResult(game.apply(command.clone())));
    }
}
