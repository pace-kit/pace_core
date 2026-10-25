use pace_core::{runtime::*, scene::Scene};
use serde_json::{json, Value};

fn game() -> Game {
    Game::new(Scene::from_json(include_str!("../assets/last-light/game.json")).unwrap())
}
fn hit(g: &mut Game, id: &str) {
    g.apply(AdventureCommand::Interact { hotspot: id.into() })
        .unwrap();
}
fn choose(g: &mut Game, id: &str) {
    g.apply(AdventureCommand::Choose { choice: id.into() })
        .unwrap();
}
fn talking() -> Game {
    let mut g = game();
    hit(&mut g, "crate");
    hit(&mut g, "workshop");
    hit(&mut g, "keeper");
    choose(&mut g, "help");
    g
}

#[test]
fn fresh_instance_resumes_every_step_to_ending() {
    let mut g = game();
    let steps = [
        (false, "crate"),
        (false, "workshop"),
        (false, "keeper"),
        (true, "help"),
        (true, "accept"),
        (false, "stairs"),
        (false, "panel"),
        (false, "beacon"),
    ];
    for (is_choice, id) in steps {
        if is_choice {
            choose(&mut g, id);
        } else {
            hit(&mut g, id);
        }
        let json = g.save_json().unwrap();
        let mut loaded = game();
        loaded.load_json(&json).unwrap();
        assert_eq!(loaded.state(), g.state(), "after {id}");
        assert_eq!(
            loaded.save_json().unwrap(),
            json,
            "deterministic serialization"
        );
        g = loaded;
    }
    assert_eq!(g.state().ending.as_deref(), Some("safe_harbor"));
    assert!(!g.state().inventory.contains("fuse"));
    assert!(g.state().inventory.contains("key"));
    assert_eq!(g.state().room, "lantern");
    assert_eq!(g.state().visited_nodes.len(), 2);
    assert_eq!(g.state().chosen_choices.len(), 2);
    assert_eq!(g.state().used_hotspots.len(), 2);
    assert!(g
        .apply(AdventureCommand::Interact {
            hotspot: "workshop".into()
        })
        .is_err());
}

#[test]
fn file_save_resumes_room_inventory_and_active_dialogue_and_overwrites_slot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slot.json");
    let original = talking();
    original.save_file(&path).unwrap();
    let mut g = game();
    g.load_file(&path).unwrap();
    assert_eq!(g.state(), original.state());
    assert_eq!(g.state().room, "workshop");
    assert!(g.state().inventory.contains("fuse"));
    assert_eq!(g.active_node().unwrap().id, "instructions");
    choose(&mut g, "accept");
    g.save_file(&path).unwrap();
    let mut reloaded = game();
    reloaded.load_file(&path).unwrap();
    assert_eq!(reloaded.state(), g.state());
    // A consumed dialogue choice and pickup stay consumed after loading.
    hit(&mut reloaded, "keeper");
    choose(&mut reloaded, "help");
    assert_eq!(reloaded.available_choices().len(), 1);
    choose(&mut reloaded, "leave");
    hit(&mut reloaded, "quay");
    assert!(reloaded.visible_hotspots().iter().all(|h| h.id != "crate"));
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "temporary file cleaned up"
    );
}

#[test]
fn incompatible_or_corrupt_loads_leave_live_state_unchanged() {
    let mut g = talking();
    let before = g.state().clone();
    let good: Value = serde_json::from_str(&g.save_json().unwrap()).unwrap();
    for (pointer, value, expected) in [
        ("/save_version", json!(99), "unsupported save version"),
        ("/game_id", json!("another_game"), "save belongs to"),
        (
            "/scene_fingerprint",
            json!("bad"),
            "scene content has changed",
        ),
        ("/state/room", json!("missing"), "$.state.room"),
        ("/state/inventory", json!(["missing"]), "$.state.inventory"),
        ("/state/flags", json!(["missing"]), "$.state.flags"),
        (
            "/state/used_hotspots",
            json!([{"room":"quay","hotspot":"workshop"}]),
            "one-shot hotspot",
        ),
        ("/state/dialogue/node", json!("missing"), "$.state.dialogue"),
        (
            "/state/dialogue/dialogue",
            json!("missing"),
            "$.state.dialogue",
        ),
        ("/state/visited_nodes", json!([]), "must have been visited"),
        (
            "/state/chosen_choices",
            json!([{"dialogue":"keeper","node":"greeting","choice":"missing"}]),
            "unknown choice",
        ),
        (
            "/state/ending",
            json!("safe_harbor"),
            "ended game cannot have active dialogue",
        ),
        ("/state/ending", json!("missing"), "unknown ending"),
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        let error = g.load_json(&bad.to_string()).unwrap_err().to_string();
        assert!(error.contains(expected), "{pointer}: {error}");
        assert_eq!(g.state(), &before, "{pointer}");
    }
    for invalid in [
        "{",
        "{}",
        "null",
        "{\"save_version\":1,\"unexpected\":true}",
    ] {
        assert!(g.load_json(invalid).is_err());
        assert_eq!(g.state(), &before);
    }
}

#[test]
fn changed_scene_rejected_but_json_formatting_and_key_order_do_not_matter() {
    let g = talking();
    let json = g.save_json().unwrap();
    let reformatted = serde_json::to_string(&serde_json::to_value(g.scene()).unwrap()).unwrap();
    let mut same = Game::new(Scene::from_json(&reformatted).unwrap());
    same.load_json(&json).unwrap();
    assert_eq!(same.state(), g.state());
    let mut scene = g.scene().clone();
    scene.rooms[0].name = "Different content".into();
    let mut changed = Game::new(scene.validate().unwrap());
    assert!(changed
        .load_json(&json)
        .unwrap_err()
        .to_string()
        .contains("fingerprint"));
}

#[test]
fn io_errors_are_reported_without_mutating_game_or_other_slots() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slot.json");
    let mut g = talking();
    g.save_file(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let before = g.state().clone();
    let missing = dir.path().join("missing/slot.json");
    assert!(g
        .save_file(&missing)
        .unwrap_err()
        .to_string()
        .contains("missing"));
    assert!(g.save_file(dir.path()).is_err()); // rename must not replace a directory
    assert!(g.load_file(&missing).is_err());
    assert_eq!(g.state(), &before);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::write(&path, "truncated{").unwrap();
    assert!(g
        .load_file(&path)
        .unwrap_err()
        .to_string()
        .contains("slot.json"));
    assert_eq!(g.state(), &before);
}
