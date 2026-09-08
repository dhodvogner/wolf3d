//! Asset loading orchestration: locating the original game data on disk and
//! falling back to a small built-in test level when it can't be found.

use std::env;
use std::path::PathBuf;

use crate::data::{GameAssets, PlayerStart, TileMap, WallTexture, load_assets};

const DEFAULT_DATA_DIRS: [&str; 2] = ["./data", "."];

/// Tries every known data directory, then falls back to a built-in level so the game always runs.
pub fn load_or_fallback_assets() -> (GameAssets, String) {
    let mut last_error = None;

    for dir in data_search_order() {
        match load_assets(&dir, 0) {
            Ok(assets) => {
                let message = format!(
                    "Loaded MAPHEAD/GAMEMAPS/VSWAP.{} from {}",
                    assets.source_extension,
                    dir.display()
                );
                return (assets, message);
            }
            Err(error) => {
                eprintln!(
                    "wolf3d: failed to load data from {}: {error}",
                    dir.display()
                );
                last_error = Some(error);
            }
        }
    }

    let fallback = fallback_assets();
    let message = match last_error {
        Some(error) => format!(
            "No original data found ({error}). Put MAPHEAD/GAMEMAPS/VSWAP in ./data or set WOLF3D_DATA_DIR."
        ),
        None => {
            "No original data found. Put MAPHEAD/GAMEMAPS/VSWAP in ./data or set WOLF3D_DATA_DIR."
                .to_string()
        }
    };
    (fallback, message)
}

fn data_search_order() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(path) = env::var("WOLF3D_DATA_DIR") {
        dirs.push(PathBuf::from(path));
    }

    for dir in DEFAULT_DATA_DIRS {
        dirs.push(PathBuf::from(dir));
    }

    dirs
}

fn fallback_assets() -> GameAssets {
    let rows = [
        "##########",
        "#........#",
        "#..##....#",
        "#........#",
        "#....#...#",
        "#........#",
        "##########",
    ];

    let width = rows[0].len();
    let height = rows.len();

    let mut walls = Vec::with_capacity(width * height);
    for row in rows {
        for ch in row.bytes() {
            walls.push(if ch == b'#' { 1 } else { 0 });
        }
    }

    let info = vec![0u16; width * height];
    let map = TileMap {
        width,
        height,
        walls,
        info,
        player_start: PlayerStart {
            x: 2.5,
            y: 2.5,
            heading: 0.0,
        },
        doors: Vec::new(),
        enemies: Vec::new(),
    };

    let texture = WallTexture {
        texels: (0..(64 * 64)).map(|i| (i % 255) as u8).collect(),
    };

    GameAssets {
        map,
        wall_textures: vec![texture],
        source_extension: "FALLBACK".to_string(),
    }
}
