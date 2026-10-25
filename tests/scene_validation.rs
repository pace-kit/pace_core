use pace_core::scene::{Scene, SCENE_VERSION};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../assets/last-light/game.json")).unwrap()
}
fn errors(v: Value) -> String {
    Scene::from_json(&v.to_string()).unwrap_err().to_string()
}

#[test]
fn fixture_validates_and_round_trips() {
    let scene = Scene::from_json(&fixture().to_string()).unwrap();
    assert_eq!(scene.scene().format_version, SCENE_VERSION);
    assert_eq!(scene.scene().rooms.len(), 3);
    Scene::from_json(&serde_json::to_string(scene.scene()).unwrap()).unwrap();
}

#[test]
fn version_checked_before_future_shape() {
    let error = errors(json!({"format_version": 999, "future_field": true}));
    assert!(error.contains("$.format_version: unsupported scene version 999"));
}

#[test]
fn syntax_types_and_unknown_fields_have_locations() {
    assert!(Scene::from_json("{\n bad")
        .unwrap_err()
        .to_string()
        .contains("line 2"));
    let mut v = fixture();
    v["rooms"][0]["hotspots"][0]["once"] = json!("yes");
    let e = errors(v);
    assert!(e.contains("rooms[0].hotspots[0].once"), "{e}");
    assert!(e.contains("boolean"), "{e}");
    let mut v = fixture();
    v["rooms"][0]["hotspots"][0]["outcome"] = json!({"type":"stay", "typo":1});
    assert!(errors(v).contains("unknown field `typo`"));
    assert!(Scene::from_json(&format!("{} trailing", fixture())).is_err());
}

#[test]
fn aggregates_duplicates_references_geometry_and_dialogue_errors() {
    let mut v = fixture();
    v["start_room"] = json!("nowhere");
    v["items"][1]["id"] = json!("fuse");
    v["rooms"][0]["hotspots"][0]["rect"] = json!([0.9, 0.0, 0.5, 0.0]);
    v["rooms"][0]["hotspots"][0]["effects"][0]["item"] = json!("missing");
    v["rooms"][0]["hotspots"][1]["outcome"]["room"] = json!("lost");
    v["dialogues"][0]["nodes"][0]["choices"][0]["next"] = json!("absent");
    v["dialogues"][0]["nodes"][1]["choices"][1]["once"] = json!(true);
    let e = errors(v);
    for expected in [
        "$.items[1].id: duplicate",
        "$.start_room: unknown room 'nowhere'",
        "$.rooms[0].hotspots[0].rect",
        "unknown item 'missing'",
        "unknown room 'lost'",
        "unknown node 'absent'",
        "unconditional, repeatable exit",
    ] {
        assert!(e.contains(expected), "missing {expected}: {e}");
    }
}

#[test]
fn all_reference_kinds_and_id_scopes_are_checked() {
    let mut v = fixture();
    v["rooms"][1]["hotspots"][1]["outcome"]["dialogue"] = json!("missing");
    v["rooms"][2]["hotspots"][2]["outcome"]["ending"] = json!("missing");
    v["rooms"][2]["hotspots"][1]["requires"][1]["flag"] = json!("missing");
    v["dialogues"][0]["nodes"][1]["choices"][0]["effects"][1]["flag"] = json!("missing");
    v["dialogues"][0]["nodes"][0]["choices"][1]["id"] = json!("help");
    v["game_id"] = json!("Bad ID");
    let e = errors(v);
    for expected in [
        "unknown dialogue",
        "unknown ending",
        "requires[1].flag",
        "effects[1].flag",
        "choices[1].id: duplicate",
        "$.game_id: id must match",
    ] {
        assert!(e.contains(expected), "{e}");
    }
}

#[test]
fn nonfinite_geometry_is_rejected_for_programmatic_scenes() {
    let mut scene = Scene::from_json(&fixture().to_string())
        .unwrap()
        .scene()
        .clone();
    scene.rooms[0].hotspots[0].rect[0] = f32::NAN;
    assert!(scene.validate().unwrap_err().to_string().contains("finite"));
}

#[test]
fn editor_schema_matches_rust_types() {
    let generated = serde_json::to_value(schemars::schema_for!(Scene)).unwrap();
    let checked_in: Value =
        serde_json::from_str(include_str!("../docs/scene-v1.schema.json")).unwrap();
    assert_eq!(
        generated, checked_in,
        "regenerate with cargo run --example scene_schema"
    );
}
