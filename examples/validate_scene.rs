use pace_core::scene::Scene;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/last-light/game.json".into());
    let json = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    match Scene::from_json(&json) {
        Ok(scene) => println!(
            "{path}: valid v{} scene '{}' ({} rooms)",
            scene.scene().format_version,
            scene.scene().game_id,
            scene.scene().rooms.len()
        ),
        Err(e) => {
            eprintln!("{path}:\n{e}");
            std::process::exit(1);
        }
    }
    Ok(())
}
