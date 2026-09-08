//! On-screen HUD text rendering, kept separate from game logic and the 3D renderer.

use macroquad::prelude::*;

use crate::fsm::GameState;
use crate::world::GameContext;

pub struct HudStats<'a> {
    pub state: GameState,
    pub player_health: i32,
    pub kills: u32,
    pub enemies_alive: usize,
    pub status_message: &'a str,
}

pub fn draw(context: &GameContext, stats: &HudStats) {
    draw_text(
        "WASD/Arrows move, E/Space use door, Ctrl/Enter fire",
        16.0,
        24.0,
        22.0,
        YELLOW,
    );
    draw_text(
        &format!(
            "State: {:?} | HP: {} | Kills: {}",
            stats.state, stats.player_health, stats.kills
        ),
        16.0,
        48.0,
        22.0,
        WHITE,
    );

    let vertical_doors = context.doors.iter().filter(|door| door.vertical).count();
    let max_lock = context
        .doors
        .iter()
        .map(|door| door.lock)
        .max()
        .unwrap_or(0);
    let info_markers = context.map.info.iter().filter(|tile| **tile > 0).count();

    draw_text(
        &format!(
            "Enemies: {} | Doors: {} (vertical {}) | Max lock {} | Info markers {}",
            stats.enemies_alive,
            context.doors.len(),
            vertical_doors,
            max_lock,
            info_markers
        ),
        16.0,
        72.0,
        20.0,
        LIGHTGRAY,
    );
    draw_text(stats.status_message, 16.0, 94.0, 20.0, LIGHTGRAY);
}
