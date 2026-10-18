//! C3 Server: runs a game project's world for its clients.
//!
//! `c3-server <project> [--map <key>]... [--cell x,y] [--port <port>] [--listen 0.0.0.0]
//! [--conquer <public ip>]`
//! serves each `--map` (a project scene's name or a content map path such as
//! `map/map/newplain.DMap`; Project Settings' start map when none is named, else the game's
//! first), with the project's NPCs, spawns, rules, scripts and quests. Any project serves:
//! the game that plays it comes from `c3-games`, its numbers from Project Settings (port,
//! tick, saving, new characters). New characters start on the first map, at `--cell` (else
//! the settings' start cell, else its middle). Maps players go to through portals are loaded
//! when first needed. Accounts and characters are kept in
//! `<project>/server/accounts`; an unknown account name logs in by making the account.
//!
//! Project Settings > Network > fronts also serve other clients; Conquer's settings serve the
//! original 5065 client (an account server on port 9958 and a game server on 5816, to which
//! logged-in clients are sent at the front's public IP). `--conquer <public ip>` serves it
//! too, at that IP.
//!
//! Type `quit` to stop (characters are saved).

use c3_core::settings::FrontSettings;
use c3_play::serve::{ServeSetup, ServedMap, ServerHandle, project_maps};
use c3_scene::CellPos;
use std::io::BufRead;
use std::sync::mpsc;
use std::time::Duration;

struct Args {
    project: String,
    maps: Vec<String>,
    cell: Option<CellPos>,
    listen: String,
    /// The engine protocol's port; `None`: Project Settings'.
    port: Option<u16>,
    /// The public IP Conquer clients reach the game server at, when serving them.
    conquer: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let project = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .ok_or("usage: c3-server <project> [--map <key>]... [--cell x,y] [--port <port>]")?;
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
        Some(p) => Some(p.parse().map_err(|_| format!("\"{p}\" is not a port"))?),
        None => None,
    };
    Ok(Args {
        project,
        maps,
        cell,
        listen: flag("--listen").unwrap_or_else(|| "0.0.0.0".into()),
        port,
        conquer: flag("--conquer"),
    })
}

fn start(args: &Args) -> Result<ServerHandle, String> {
    let project = c3_project::Project::open(&args.project).map_err(|e| e.to_string())?;
    let (game, asset_problems) = c3_games::open_project(&project)?;
    for problem in asset_problems {
        eprintln!("warning: {problem}");
    }
    println!("Game: {}", game.name());
    let (mut settings, problem) = c3_play::settings::load(&project, &*game);
    if let Some(problem) = problem {
        eprintln!("warning: {problem}");
    }
    if let Some(ip) = &args.conquer {
        settings
            .network
            .fronts
            .retain(|f| f.kind != c3_games::CONQUER_FRONT);
        settings.network.fronts.push(FrontSettings {
            kind: c3_games::CONQUER_FRONT.into(),
            public_ip: ip.clone(),
        });
    }
    let mut keys = args.maps.clone();
    if keys.is_empty() {
        keys.push(
            c3_play::settings::start_map(&settings.gameplay, &project, &*game)
                .ok_or("the project has no map to serve; make one in the Studio")?,
        );
    }
    let list = c3_play::map_list::MapList::new(&project, &*game, &settings);
    let mut maps = Vec::new();
    for (i, key) in keys.into_iter().enumerate() {
        let scene = game.load_map(&project, &key)?;
        let middle = (scene.grid.width as i32 / 2, scene.grid.height as i32 / 2);
        let start = if i == 0 {
            (args.cell).unwrap_or_else(|| c3_play::settings::start_cell(&settings.gameplay, &scene))
        } else {
            middle
        };
        println!(
            "Map {key}: {} NPCs, {} spawn areas",
            scene.npcs.len(),
            scene.spawns.len()
        );
        maps.push(ServedMap {
            map_ids: list
                .by_key(&key)
                .map_or_else(|| game.map_ids(&key), |m| m.ids.clone()),
            key,
            scene,
            start,
        });
    }
    ServerHandle::spawn(
        maps,
        ServeSetup {
            listen: format!(
                "{}:{}",
                args.listen,
                args.port.unwrap_or(settings.network.port)
            ),
            scripts: Some(project.scripts_dir()),
            tables: Some(project.tables_dir()),
            accounts: project.server_dir().join("accounts"),
            rules: game.rules(),
            fronts: c3_games::fronts(&settings.network, &args.listen)?,
            more_maps: Some(project_maps(project.clone(), settings.clone(), open_game)),
            settings,
        },
    )
}

/// The game that plays `project`, for the server's thread.
fn open_game(
    project: &c3_project::Project,
) -> Result<std::rc::Rc<dyn c3_play::GameModule>, String> {
    c3_games::open_project(project).map(|(game, _)| game)
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
