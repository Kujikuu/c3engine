//! C3 Server: runs a game project's world for its clients.
//!
//! `c3-server <project> [--map <key>]... [--cell x,y] [--port 7101] [--listen 0.0.0.0]`
//! serves each `--map` (a project scene's name or a content map path such as
//! `map/map/newplain.DMap`; Twin City when none is named), with the project's NPCs, spawns,
//! rules, scripts and quests. New characters start on the first map, at `--cell` (else its
//! middle). Accounts and characters are kept in `<project>/server/accounts`; an unknown
//! account name logs in by making the account.
//!
//! Type `online` to see who is playing, `quit` to stop (characters are saved).

use c3_scene::CellPos;
use game_conquer::Content;
use game_conquer::serve::{ServeSetup, ServedMap, ServerHandle};
use std::io::BufRead;
use std::sync::mpsc;
use std::time::Duration;

/// The engine protocol's port.
const DEFAULT_PORT: u16 = 7101;
/// Twin City.
const DEFAULT_MAP_ID: u32 = 1002;

struct Args {
    project: String,
    maps: Vec<String>,
    cell: Option<CellPos>,
    listen: String,
    port: u16,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let project = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .ok_or("usage: c3-server <project> [--map <key>]... [--cell x,y] [--port 7101]")?;
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let maps = args
        .windows(2)
        .filter(|w| w[0] == "--map")
        .map(|w| w[1].clone())
        .collect();
    let cell = flag("--cell").and_then(|c| {
        let (x, y) = c.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    });
    let port = match flag("--port") {
        Some(p) => p.parse().map_err(|_| format!("\"{p}\" is not a port"))?,
        None => DEFAULT_PORT,
    };
    Ok(Args {
        project,
        maps,
        cell,
        listen: flag("--listen").unwrap_or_else(|| "0.0.0.0".into()),
        port,
    })
}

fn start(args: &Args) -> Result<ServerHandle, String> {
    let project = c3_project::Project::open(&args.project).map_err(|e| e.to_string())?;
    let (_, content_dir) = project
        .tq_client()
        .ok_or("the project has no Conquer content to serve")?;
    let content = Content::open(content_dir)?;
    let mut keys = args.maps.clone();
    if keys.is_empty() {
        keys.push(
            content
                .map_path(DEFAULT_MAP_ID)
                .ok_or(format!("map {DEFAULT_MAP_ID} is not in GameMap.dat"))?,
        );
    }
    let mut maps = Vec::new();
    for (i, key) in keys.into_iter().enumerate() {
        let scene = game_conquer::game::load_map(&project, &content, &key)?;
        let middle = (scene.grid.width as i32 / 2, scene.grid.height as i32 / 2);
        let start = if i == 0 {
            args.cell.unwrap_or(middle)
        } else {
            middle
        };
        println!(
            "Map {key}: {} NPCs, {} spawn areas",
            scene.npcs.len(),
            scene.spawns.len()
        );
        maps.push(ServedMap {
            map_ids: content.map_ids_for_path(&key),
            key,
            scene,
            start,
        });
    }
    ServerHandle::spawn(
        maps,
        ServeSetup {
            listen: format!("{}:{}", args.listen, args.port),
            scripts: Some(project.scripts_dir()),
            tables: Some(project.tables_dir()),
            accounts: project.server_dir().join("accounts"),
        },
    )
}

fn main() {
    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let mut server = start(&args).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });
    println!("Listening on {}. Type `quit` to stop.", server.addr());
    let (commands, typed) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if commands.send(line).is_err() {
                break;
            }
        }
    });
    while server.is_running() {
        for line in server.take_log() {
            println!("{line}");
        }
        match typed.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => match line.trim() {
                "quit" | "exit" | "stop" => break,
                "" => {}
                other => println!("Unknown command \"{other}\" (quit)"),
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // No console (run in the background): just serve.
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(200))
            }
        }
    }
    server.stop();
    for line in server.take_log() {
        println!("{line}");
    }
}
