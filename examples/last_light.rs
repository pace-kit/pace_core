//! A minimal point-and-click player for any validated v1 scene.
use bevy::{
    picking::hover::Hovered,
    prelude::*,
    ui_widgets::{Activate, Button},
};
use pace_core::{runtime::*, scene::Scene};

const INK: Color = Color::srgb(0.035, 0.055, 0.075);
const PAPER: Color = Color::srgb(0.94, 0.91, 0.82);
const GOLD: Color = Color::srgb(0.92, 0.68, 0.32);
const PANEL: Color = Color::srgb(0.1, 0.15, 0.18);

#[derive(Component)]
struct Screen;
#[derive(Resource)]
struct Status(String);
#[derive(Message, Clone)]
enum PlayerInput {
    Act(AdventureCommand),
    Restart,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?,
        None => include_str!("../assets/last-light/game.json").into(),
    };
    let game = Game::new(Scene::from_json(&json)?);
    let title = game.scene().title.clone();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title,
                resolution: (1120, 820).into(),
                resize_constraints: bevy::window::WindowResizeConstraints {
                    min_width: 960.0,
                    min_height: 760.0,
                    ..default()
                },
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(INK))
        .insert_resource(game)
        .insert_resource(Status(
            "Click a labeled hotspot, or press its number. Bring the boat home.".into(),
        ))
        .add_message::<PlayerInput>()
        .add_plugins(AdventurePlugin)
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(
            Update,
            (keyboard, player_input).chain().before(AdventureStep),
        )
        .add_systems(Update, (feedback, redraw).chain().after(AdventureStep))
        .add_systems(Update, hover)
        .run();
    Ok(())
}

fn keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    game: Res<Game>,
    mut input: MessageWriter<PlayerInput>,
) {
    if keys.just_pressed(KeyCode::KeyN) {
        input.write(PlayerInput::Restart);
        return;
    }
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    for (i, key) in digits.iter().enumerate() {
        if !keys.just_pressed(*key) {
            continue;
        }
        let action = if game.state().dialogue.is_some() {
            game.available_choices()
                .get(i)
                .map(|c| AdventureCommand::Choose {
                    choice: c.id.clone(),
                })
        } else {
            game.visible_hotspots()
                .get(i)
                .map(|h| AdventureCommand::Interact {
                    hotspot: h.id.clone(),
                })
        };
        if let Some(action) = action {
            input.write(PlayerInput::Act(action));
        }
        break;
    }
}
fn player_input(
    mut input: MessageReader<PlayerInput>,
    mut actions: MessageWriter<AdventureCommand>,
    mut game: ResMut<Game>,
    mut status: ResMut<Status>,
) {
    // At most one UI action per frame, avoiding stale double-click/keyboard actions.
    if let Some(input) = input.read().next() {
        match input {
            PlayerInput::Act(action) => {
                actions.write(action.clone());
            }
            PlayerInput::Restart => {
                *game = Game::new(game.scene().clone().validate().expect("already validated"));
                status.0 = "A new evening on the quay. Find a way to light the beacon.".into();
            }
        }
    }
    input.clear();
}
fn feedback(mut results: MessageReader<AdventureResult>, mut status: ResMut<Status>) {
    for result in results.read() {
        status.0 = match &result.0 {
            Ok(text) => text.clone(),
            Err(e) => e.to_string(),
        };
    }
}
fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}
fn button(parent: &mut ChildSpawnerCommands, text: &str, action: PlayerInput, node: Node) {
    parent
        .spawn((
            Button,
            Hovered::default(),
            node,
            BackgroundColor(PANEL),
            BorderColor::all(GOLD),
        ))
        .observe(
            move |_: On<Activate>, mut input: MessageWriter<PlayerInput>| {
                input.write(action.clone());
            },
        )
        .with_children(|p| {
            p.spawn(label(text, 17.0, PAPER));
        });
}
fn button_node() -> Node {
    Node {
        padding: UiRect::all(px(14)),
        border: UiRect::all(px(1)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}
fn hover(mut buttons: Query<(&Hovered, &mut BackgroundColor), With<Button>>) {
    for (hovered, mut color) in &mut buttons {
        let wanted = if hovered.get() {
            Color::srgb(0.2, 0.3, 0.33)
        } else {
            PANEL
        };
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
}
fn redraw(
    mut commands: Commands,
    game: Res<Game>,
    status: Res<Status>,
    roots: Query<Entity, With<Screen>>,
) {
    if !game.is_changed() && !status.is_changed() {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    let room = game.room();
    commands.spawn((Screen, Node {
        width: percent(100), height: percent(100), padding: UiRect::all(px(28)),
        flex_direction: FlexDirection::Column, row_gap: px(14), ..default()
    }, BackgroundColor(INK))).with_children(|root| {
        root.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() })
            .with_children(|header| {
                header.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }).with_children(|p| {
                    p.spawn(label("PACE / A THREE-ROOM ADVENTURE", 13.0, GOLD));
                    p.spawn(label(&game.scene().title, 32.0, PAPER));
                });
                button(header, "New game [N]", PlayerInput::Restart, button_node());
            });
        root.spawn(label(&room.name, 24.0, GOLD));
        root.spawn((label(&room.description, 17.0, PAPER), Node { min_height: px(48), ..default() }));
        root.spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(320), column_gap: px(20), ..default() }).with_children(|body| {
            body.spawn((Node { width: percent(73), height: percent(100), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgb_u8(room.color[0], room.color[1], room.color[2])))).with_children(|canvas| {
                // Original geometric room art. Authored rectangles are the actual click targets.
                canvas.spawn((Node { position_type: PositionType::Absolute, left: percent(0), top: percent(86), width: percent(100), height: percent(14), ..default() }, BackgroundColor(PANEL)));
                canvas.spawn((label("THE LAST LIGHT / NORTH REEF STATION", 12.0, PAPER), Node { position_type: PositionType::Absolute, left: px(18), top: px(16), ..default() }));
                if let Some(ending) = &game.state().ending {
                    let ending = game.scene().endings.iter().find(|e| &e.id == ending).unwrap();
                    canvas.spawn(Node { padding: UiRect::all(px(40)), margin: UiRect::top(px(45)), flex_direction: FlexDirection::Column, row_gap: px(24), ..default() }).with_children(|p| {
                        p.spawn(label("SAFE HARBOR", 32.0, GOLD));
                        p.spawn(label(&ending.text, 22.0, PAPER));
                    });
                } else if let Some(node) = game.active_node() {
                    canvas.spawn(Node { width: percent(100), padding: UiRect::all(px(30)), margin: UiRect::top(px(30)), flex_direction: FlexDirection::Column, row_gap: px(18), ..default() }).with_children(|p| {
                        p.spawn(label(&node.speaker, 24.0, GOLD));
                        p.spawn(label(&node.text, 19.0, PAPER));
                        for (i, c) in game.available_choices().iter().enumerate() {
                            button(p, &format!("{}. {}", i + 1, c.text), PlayerInput::Act(AdventureCommand::Choose { choice: c.id.clone() }), button_node());
                        }
                    });
                } else {
                    for (i, h) in game.visible_hotspots().iter().enumerate() {
                        let [x, y, w, height] = h.rect;
                        button(canvas, &format!("{}. {}", i + 1, h.label), PlayerInput::Act(AdventureCommand::Interact { hotspot: h.id.clone() }), Node {
                            position_type: PositionType::Absolute, left: percent(x * 100.0), top: percent(y * 100.0), width: percent(w * 100.0), height: percent(height * 100.0), ..button_node()
                        });
                    }
                }
            });
            body.spawn((Node { width: percent(27), padding: UiRect::all(px(18)), flex_direction: FlexDirection::Column, row_gap: px(18), ..default() }, BackgroundColor(PANEL))).with_children(|p| {
                p.spawn(label("INVENTORY", 17.0, GOLD));
                if game.state().inventory.is_empty() { p.spawn(label("Your pockets are empty.", 16.0, PAPER)); }
                for item in game.scene().items.iter().filter(|i| game.state().inventory.contains(&i.id)) {
                    p.spawn(label(&item.name, 19.0, GOLD));
                    p.spawn(label(&item.description, 15.0, PAPER));
                }
                p.spawn((label("Click a hotspot or press 1-9. Items are used automatically where needed. Dialogue choices appear in the room panel.", 14.0, PAPER), Node { margin: UiRect::top(px(20)), ..default() }));
            });
        });
        root.spawn((label(&status.0, 17.0, PAPER), Node { min_height: px(48), padding: UiRect::all(px(12)), ..default() }, BackgroundColor(PANEL)));
        root.spawn(label("THE LAST LIGHT  /  Original story & geometric artwork  /  PACE", 12.0, GOLD));
    });
}
