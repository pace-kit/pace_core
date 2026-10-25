//! Terminal player and reproducible, self-checking walkthrough of the fixture.
use pace_core::{runtime::*, scene::Scene};
use std::{
    io::{self, BufRead, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn new_game() -> Game {
    Game::new(
        Scene::from_json(include_str!("../assets/last-light/game.json"))
            .expect("validated fixture"),
    )
}

fn show(game: &Game) {
    println!("\n{}\n{}", game.room().name, game.room().description);
    let inventory: Vec<_> = game
        .scene()
        .items
        .iter()
        .filter(|i| game.state().inventory.contains(&i.id))
        .map(|i| i.name.as_str())
        .collect();
    println!(
        "Inventory: {}",
        if inventory.is_empty() {
            "(empty)".into()
        } else {
            inventory.join(", ")
        }
    );
    if let Some(ending) = &game.state().ending {
        println!(
            "{}",
            game.scene()
                .endings
                .iter()
                .find(|e| &e.id == ending)
                .unwrap()
                .text
        );
    } else if let Some(node) = game.active_node() {
        println!("{}: {}", node.speaker, node.text);
        for (i, c) in game.available_choices().iter().enumerate() {
            println!("  {}. {} [choose {}]", i + 1, c.text, c.id);
        }
    } else {
        for (i, h) in game.visible_hotspots().iter().enumerate() {
            println!("  {}. {} [click {}]", i + 1, h.label, h.id);
        }
    }
}

fn execute(game: &mut Game, line: &str, slot: &Path) -> Result<String> {
    let words: Vec<_> = line.split_whitespace().collect();
    match words.as_slice() {
        ["click", id] => Ok(game.apply(AdventureCommand::Interact {
            hotspot: (*id).into(),
        })?),
        ["choose", id] => Ok(game.apply(AdventureCommand::Choose {
            choice: (*id).into(),
        })?),
        ["save"] => {
            game.save_file(slot)?;
            Ok("Saved checkpoint.".into())
        }
        ["load"] => {
            game.load_file(slot)?;
            Ok("Loaded checkpoint.".into())
        }
        ["new"] => {
            *game = new_game();
            Ok("Started a new game.".into())
        }
        ["look"] => Ok(String::new()),
        [number] if number.parse::<usize>().is_ok() => {
            let i = number
                .parse::<usize>()?
                .checked_sub(1)
                .ok_or("Numbers start at 1")?;
            let command = if game.state().dialogue.is_some() {
                AdventureCommand::Choose {
                    choice: game
                        .available_choices()
                        .get(i)
                        .ok_or("No choice at that number")?
                        .id
                        .clone(),
                }
            } else {
                AdventureCommand::Interact {
                    hotspot: game
                        .visible_hotspots()
                        .get(i)
                        .ok_or("No hotspot at that number")?
                        .id
                        .clone(),
                }
            };
            Ok(game.apply(command)?)
        }
        _ => Err("Use a number, click ID, choose ID, look, save, load, new, or quit".into()),
    }
}

fn play(reader: impl BufRead, game: &mut Game, slot: &Path, walkthrough: bool) -> Result<()> {
    let mut checkpoint = None;
    show(game);
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        println!("\n> {line}");
        if line == "quit" {
            break;
        }
        match execute(game, line, slot) {
            Ok(text) => println!("{text}"),
            Err(e) if walkthrough => {
                return Err(format!("walkthrough command '{line}' failed: {e}").into())
            }
            Err(e) => eprintln!("{e}"),
        }
        if walkthrough {
            if line == "save" {
                checkpoint = Some(game.state().clone());
            }
            if line == "load" {
                assert_eq!(
                    Some(game.state()),
                    checkpoint.as_ref(),
                    "complete saved state must survive a fresh game"
                );
                println!("Verified: room, inventory, flags, dialogue, history and ending match checkpoint.");
            }
        }
        show(game);
        io::stdout().flush()?;
    }
    if walkthrough {
        assert_eq!(game.state().ending.as_deref(), Some("safe_harbor"));
        assert_eq!(game.state().room, "lantern");
        assert!(game.state().inventory.contains("key"));
        assert!(!game.state().inventory.contains("fuse"));
        println!("\nWALKTHROUGH PASS: safe_harbor reached and restored from disk.");
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    println!(
        "THE LAST LIGHT / PACE\nCommands: number, click ID, choose ID, look, save, load, new, quit"
    );
    let mut game = new_game();
    if args.as_slice() == ["--walkthrough"] {
        // Never overwrite a player's slot during the automated demonstration.
        let dir = tempfile::tempdir()?;
        play(
            io::Cursor::new(include_str!("../assets/last-light/walkthrough.txt")),
            &mut game,
            &dir.path().join("demo.json"),
            true,
        )
    } else if args.is_empty() {
        play(
            io::stdin().lock(),
            &mut game,
            Path::new("last_light.save.json"),
            false,
        )
    } else {
        Err("Usage: cargo run --example console -- [--walkthrough]".into())
    }
}
