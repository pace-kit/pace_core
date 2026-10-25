//! Versioned, renderer-independent scene data and editor-facing validation.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

pub const SCENE_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub format_version: u32,
    pub game_id: String,
    pub title: String,
    pub start_room: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub flags: Vec<String>,
    pub rooms: Vec<Room>,
    #[serde(default)]
    pub dialogues: Vec<Dialogue>,
    pub endings: Vec<Ending>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: String,
    pub color: [u8; 3],
    pub hotspots: Vec<Hotspot>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hotspot {
    pub id: String,
    pub label: String,
    /// Normalized top-left x, y, width, height in the room's viewport.
    pub rect: [f32; 4],
    #[serde(default)]
    pub once: bool,
    #[serde(default)]
    pub requires: Vec<Condition>,
    pub blocked_text: String,
    pub text: String,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub outcome: Outcome,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    HasItem { item: String },
    Flag { flag: String, value: bool },
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    GiveItem { item: String },
    RemoveItem { item: String },
    SetFlag { flag: String, value: bool },
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Stay {},
    Room { room: String },
    Dialogue { dialogue: String },
    End { ending: String },
}

impl Default for Outcome {
    fn default() -> Self {
        Self::Stay {}
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dialogue {
    pub id: String,
    pub start: String,
    pub nodes: Vec<DialogueNode>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DialogueNode {
    pub id: String,
    pub speaker: String,
    pub text: String,
    pub choices: Vec<Choice>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub once: bool,
    #[serde(default)]
    pub requires: Vec<Condition>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// null (or omitted) closes dialogue; otherwise a node in this dialogue.
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ending {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostics(pub Vec<Diagnostic>);

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, d) in self.0.iter().enumerate() {
            if i != 0 {
                writeln!(f)?;
            }
            write!(f, "{}: {}", d.path, d.message)?;
        }
        Ok(())
    }
}
impl std::error::Error for Diagnostics {}

/// Only validated scenes can enter the runtime. Mutation requires revalidation.
#[derive(Clone, Debug)]
pub struct ValidatedScene(Scene);

impl ValidatedScene {
    pub fn scene(&self) -> &Scene {
        &self.0
    }
}

impl Scene {
    pub fn from_json(json: &str) -> Result<ValidatedScene, Diagnostics> {
        // Check the version before interpreting a future version's shape.
        let header: serde_json::Value = serde_json::from_str(json).map_err(|e| {
            Diagnostics(vec![Diagnostic {
                path: "$".into(),
                message: e.to_string(),
            }])
        })?;
        if let Some(version) = header.get("format_version").and_then(|v| v.as_u64()) {
            if version != u64::from(SCENE_VERSION) {
                return Err(Diagnostics(vec![Diagnostic {
                    path: "$.format_version".into(),
                    message: format!(
                        "unsupported scene version {version}; expected {SCENE_VERSION}"
                    ),
                }]));
            }
        }
        let mut de = serde_json::Deserializer::from_str(json);
        let scene: Scene = serde_path_to_error::deserialize(&mut de).map_err(|e| {
            Diagnostics(vec![Diagnostic {
                path: format!("$.{}", e.path()),
                message: e.inner().to_string(),
            }])
        })?;
        scene.validate()
    }

    pub fn validate(self) -> Result<ValidatedScene, Diagnostics> {
        let mut v = Validator::default();
        if self.format_version != SCENE_VERSION {
            v.error(
                "$.format_version",
                format!(
                    "unsupported scene version {}; expected {SCENE_VERSION}",
                    self.format_version
                ),
            );
        }
        v.id("$.game_id", &self.game_id);
        v.text("$.title", &self.title);
        let rooms = v.ids("$.rooms", self.rooms.iter().map(|r| r.id.as_str()));
        let items = v.ids("$.items", self.items.iter().map(|i| i.id.as_str()));
        let dialogues = v.ids("$.dialogues", self.dialogues.iter().map(|d| d.id.as_str()));
        let endings = v.ids("$.endings", self.endings.iter().map(|e| e.id.as_str()));
        let mut flags = BTreeSet::new();
        for (i, flag) in self.flags.iter().enumerate() {
            let p = format!("$.flags[{i}]");
            v.id(&p, flag);
            if !flags.insert(flag.as_str()) {
                v.error(&p, format!("duplicate id '{flag}'"));
            }
        }
        if rooms.is_empty() {
            v.error("$.rooms", "at least one room is required");
        }
        if endings.is_empty() {
            v.error("$.endings", "at least one ending is required");
        }
        v.reference("$.start_room", &self.start_room, &rooms, "room");
        for (i, item) in self.items.iter().enumerate() {
            v.text(&format!("$.items[{i}].name"), &item.name);
            v.text(&format!("$.items[{i}].description"), &item.description);
        }
        for (i, ending) in self.endings.iter().enumerate() {
            v.text(&format!("$.endings[{i}].text"), &ending.text);
        }
        for (i, room) in self.rooms.iter().enumerate() {
            let p = format!("$.rooms[{i}]");
            v.text(&format!("{p}.name"), &room.name);
            v.text(&format!("{p}.description"), &room.description);
            v.ids(
                &format!("{p}.hotspots"),
                room.hotspots.iter().map(|h| h.id.as_str()),
            );
            for (j, h) in room.hotspots.iter().enumerate() {
                let p = format!("{p}.hotspots[{j}]");
                v.text(&format!("{p}.label"), &h.label);
                v.text(&format!("{p}.text"), &h.text);
                v.text(&format!("{p}.blocked_text"), &h.blocked_text);
                let [x, y, w, height] = h.rect;
                if !h.rect.iter().all(|n| n.is_finite())
                    || x < 0.0
                    || y < 0.0
                    || w <= 0.0
                    || height <= 0.0
                    || x + w > 1.0
                    || y + height > 1.0
                {
                    v.error(
                        &format!("{p}.rect"),
                        "expected finite [x, y, width, height], positive size, within [0, 1]",
                    );
                }
                v.conditions(&format!("{p}.requires"), &h.requires, &items, &flags);
                v.effects(&format!("{p}.effects"), &h.effects, &items, &flags);
                match &h.outcome {
                    Outcome::Stay {} => {}
                    Outcome::Room { room } => {
                        v.reference(&format!("{p}.outcome.room"), room, &rooms, "room")
                    }
                    Outcome::Dialogue { dialogue } => v.reference(
                        &format!("{p}.outcome.dialogue"),
                        dialogue,
                        &dialogues,
                        "dialogue",
                    ),
                    Outcome::End { ending } => {
                        v.reference(&format!("{p}.outcome.ending"), ending, &endings, "ending")
                    }
                }
            }
        }
        for (i, d) in self.dialogues.iter().enumerate() {
            let p = format!("$.dialogues[{i}]");
            let nodes = v.ids(&format!("{p}.nodes"), d.nodes.iter().map(|n| n.id.as_str()));
            v.reference(&format!("{p}.start"), &d.start, &nodes, "node");
            for (j, n) in d.nodes.iter().enumerate() {
                let p = format!("{p}.nodes[{j}]");
                v.text(&format!("{p}.speaker"), &n.speaker);
                v.text(&format!("{p}.text"), &n.text);
                v.ids(
                    &format!("{p}.choices"),
                    n.choices.iter().map(|c| c.id.as_str()),
                );
                // A repeatable, unconditional exit prevents getting trapped after one-shot choices.
                if !n
                    .choices
                    .iter()
                    .any(|c| c.next.is_none() && c.requires.is_empty() && !c.once)
                {
                    v.error(
                        &format!("{p}.choices"),
                        "each node needs an unconditional, repeatable exit (next: null)",
                    );
                }
                for (k, c) in n.choices.iter().enumerate() {
                    let p = format!("{p}.choices[{k}]");
                    v.text(&format!("{p}.text"), &c.text);
                    v.conditions(&format!("{p}.requires"), &c.requires, &items, &flags);
                    v.effects(&format!("{p}.effects"), &c.effects, &items, &flags);
                    if let Some(next) = &c.next {
                        v.reference(&format!("{p}.next"), next, &nodes, "node");
                    }
                }
            }
        }
        if v.errors.is_empty() {
            Ok(ValidatedScene(self))
        } else {
            Err(Diagnostics(v.errors))
        }
    }
}

#[derive(Default)]
struct Validator {
    errors: Vec<Diagnostic>,
}

impl Validator {
    fn error(&mut self, path: &str, message: impl Into<String>) {
        self.errors.push(Diagnostic {
            path: path.into(),
            message: message.into(),
        });
    }
    fn text(&mut self, path: &str, text: &str) {
        if text.trim().is_empty() {
            self.error(path, "must not be empty");
        }
    }
    fn id(&mut self, path: &str, id: &str) {
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            self.error(path, "id must match [a-z0-9_]+ (stable, case-sensitive)");
        }
    }
    fn ids<'a>(&mut self, path: &str, ids: impl Iterator<Item = &'a str>) -> BTreeSet<&'a str> {
        let mut found = BTreeSet::new();
        for (i, id) in ids.enumerate() {
            let p = format!("{path}[{i}].id");
            self.id(&p, id);
            if !found.insert(id) {
                self.error(&p, format!("duplicate id '{id}'"));
            }
        }
        found
    }
    fn reference(&mut self, path: &str, id: &str, ids: &BTreeSet<&str>, kind: &str) {
        if !ids.contains(id) {
            self.error(path, format!("unknown {kind} '{id}'"));
        }
    }
    fn conditions(
        &mut self,
        path: &str,
        conditions: &[Condition],
        items: &BTreeSet<&str>,
        flags: &BTreeSet<&str>,
    ) {
        for (i, c) in conditions.iter().enumerate() {
            match c {
                Condition::HasItem { item } => {
                    self.reference(&format!("{path}[{i}].item"), item, items, "item")
                }
                Condition::Flag { flag, .. } => {
                    self.reference(&format!("{path}[{i}].flag"), flag, flags, "flag")
                }
            }
        }
    }
    fn effects(
        &mut self,
        path: &str,
        effects: &[Effect],
        items: &BTreeSet<&str>,
        flags: &BTreeSet<&str>,
    ) {
        for (i, e) in effects.iter().enumerate() {
            match e {
                Effect::GiveItem { item } | Effect::RemoveItem { item } => {
                    self.reference(&format!("{path}[{i}].item"), item, items, "item")
                }
                Effect::SetFlag { flag, .. } => {
                    self.reference(&format!("{path}[{i}].flag"), flag, flags, "flag")
                }
            }
        }
    }
}
