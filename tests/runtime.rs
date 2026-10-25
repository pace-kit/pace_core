use bevy::prelude::*;
use pace_core::{runtime::*, scene::Scene};

fn game() -> Game {
    Game::new(Scene::from_json(include_str!("../assets/last-light/game.json")).unwrap())
}
fn hit(game: &mut Game, id: &str) -> Result<String, ActionError> {
    game.apply(AdventureCommand::Interact { hotspot: id.into() })
}
fn choose(game: &mut Game, id: &str) -> Result<String, ActionError> {
    game.apply(AdventureCommand::Choose { choice: id.into() })
}
fn key(game: &mut Game) {
    hit(game, "keeper").unwrap();
    choose(game, "help").unwrap();
    choose(game, "accept").unwrap();
}

#[test]
fn complete_game_and_terminal_state() {
    let mut g = game();
    hit(&mut g, "crate").unwrap();
    hit(&mut g, "workshop").unwrap();
    key(&mut g);
    hit(&mut g, "stairs").unwrap();
    assert!(hit(&mut g, "beacon").is_err());
    hit(&mut g, "panel").unwrap();
    assert!(!g.state().inventory.contains("fuse"));
    assert!(g.state().inventory.contains("key"));
    assert!(g.state().flags.contains("repaired"));
    hit(&mut g, "beacon").unwrap();
    assert_eq!(g.state().ending.as_deref(), Some("safe_harbor"));
    let ended = g.state().clone();
    assert_eq!(hit(&mut g, "workshop"), Err(ActionError::Finished));
    assert_eq!(choose(&mut g, "help"), Err(ActionError::Finished));
    assert_eq!(g.state(), &ended);
}

#[test]
fn failed_actions_are_atomic_and_do_not_consume_pickups() {
    let mut g = game();
    let before = g.state().clone();
    assert!(hit(&mut g, "nonexistent").is_err());
    assert_eq!(choose(&mut g, "help"), Err(ActionError::NoDialogue));
    assert_eq!(g.state(), &before);
    hit(&mut g, "workshop").unwrap();
    let before = g.state().clone();
    assert!(hit(&mut g, "stairs")
        .unwrap_err()
        .to_string()
        .contains("brass key"));
    assert_eq!(g.state(), &before);
    key(&mut g);
    hit(&mut g, "stairs").unwrap();
    let before = g.state().clone();
    assert!(hit(&mut g, "panel").is_err());
    assert_eq!(g.state(), &before);
    // Recover after arriving without the fuse; the puzzle remains solvable.
    hit(&mut g, "workshop").unwrap();
    hit(&mut g, "quay").unwrap();
    hit(&mut g, "crate").unwrap();
    let before = g.state().clone();
    assert!(hit(&mut g, "crate").is_err());
    assert_eq!(g.state(), &before);
    assert!(g.visible_hotspots().iter().all(|h| h.id != "crate"));
    hit(&mut g, "workshop").unwrap();
    hit(&mut g, "stairs").unwrap();
    hit(&mut g, "panel").unwrap();
    hit(&mut g, "beacon").unwrap();
}

#[test]
fn dialogue_is_modal_and_remembers_once_choices() {
    let mut g = game();
    hit(&mut g, "workshop").unwrap();
    hit(&mut g, "keeper").unwrap();
    assert!(g.visible_hotspots().is_empty());
    let before = g.state().clone();
    assert_eq!(hit(&mut g, "quay"), Err(ActionError::DialogueActive));
    assert!(choose(&mut g, "no_such_choice").is_err());
    assert_eq!(g.state(), &before);
    choose(&mut g, "help").unwrap();
    assert_eq!(g.state().dialogue.as_ref().unwrap().node, "instructions");
    choose(&mut g, "accept").unwrap();
    assert_eq!(g.state().visited_nodes.len(), 2);
    assert_eq!(g.state().chosen_choices.len(), 2);
    hit(&mut g, "keeper").unwrap();
    choose(&mut g, "help").unwrap();
    assert_eq!(
        g.available_choices()
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["leave"]
    );
    let before = g.state().clone();
    assert!(choose(&mut g, "accept").is_err());
    assert_eq!(g.state(), &before);
    choose(&mut g, "leave").unwrap();
    assert!(g.state().dialogue.is_none());
}

#[test]
fn hit_testing_matches_rectangles_and_authored_order() {
    let mut scene = game().scene().clone();
    scene.rooms[0].hotspots[1].rect = scene.rooms[0].hotspots[0].rect;
    let g = Game::new(scene.validate().unwrap());
    assert_eq!(g.hotspot_at(0.1, 0.6).unwrap().id, "workshop");
    assert!(g.hotspot_at(-1.0, 0.6).is_none());
    assert!(g.hotspot_at(f32::NAN, 0.6).is_none());
    assert!(g.hotspot_at(1.0, 1.0).is_none());
    let g = game();
    let [x, y, w, h] = g.room().hotspots[0].rect;
    assert_eq!(g.hotspot_at(x, y).unwrap().id, "crate");
    assert!(g.hotspot_at(x + w, y).is_none());
    assert!(g.hotspot_at(x, y + h).is_none());
}

#[test]
fn conditions_effect_order_and_false_flags() {
    use pace_core::scene::{Condition, Effect};
    let mut scene = game().scene().clone();
    scene.rooms[0].hotspots[0].effects = vec![
        Effect::GiveItem {
            item: "fuse".into(),
        },
        Effect::GiveItem {
            item: "fuse".into(),
        },
        Effect::SetFlag {
            flag: "briefed".into(),
            value: true,
        },
        Effect::SetFlag {
            flag: "briefed".into(),
            value: false,
        },
    ];
    scene.dialogues[0].nodes[0].choices[0].requires = vec![Condition::Flag {
        flag: "briefed".into(),
        value: true,
    }];
    let mut g = Game::new(scene.validate().unwrap());
    hit(&mut g, "crate").unwrap();
    assert_eq!(g.state().inventory.len(), 1);
    assert!(g.meets(&[Condition::Flag {
        flag: "briefed".into(),
        value: false
    }]));
    hit(&mut g, "workshop").unwrap();
    hit(&mut g, "keeper").unwrap();
    assert_eq!(g.available_choices().len(), 1);
    let before = g.state().clone();
    assert!(choose(&mut g, "help").is_err());
    assert_eq!(g.state(), &before);
}

#[test]
fn bevy_plugin_processes_commands_in_order_without_a_renderer() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(game())
        .add_plugins(AdventurePlugin);
    for id in ["crate", "workshop", "keeper"] {
        app.world_mut()
            .write_message(AdventureCommand::Interact { hotspot: id.into() });
    }
    app.update();
    let g = app.world().resource::<Game>();
    assert_eq!(g.state().room, "workshop");
    assert!(g.state().inventory.contains("fuse"));
    assert_eq!(g.state().dialogue.as_ref().unwrap().node, "greeting");
}
