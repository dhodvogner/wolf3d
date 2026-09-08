//! Game world simulation: player/entity transforms, collision and doors.
//!
//! `GameContext` is the single source of truth for spatial state. Commands
//! (see [`crate::command`]) mutate it, and systems (enemy AI, rendering)
//! read from it. Keeping collision and door logic here avoids scattering
//! world-query code across the app layer.

use crate::data::{DoorSpawn, TileMap};
use crate::ecs::{Entity, Transform, World};

#[derive(Clone, Copy, PartialEq, Eq)]
enum DoorState {
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Clone, Copy)]
pub struct DoorRuntime {
    pub x: usize,
    pub y: usize,
    pub vertical: bool,
    pub lock: u8,
    pub tile: u16,
    pub open_ratio: f32,
    state: DoorState,
    open_timer: f32,
}

/// Converts static map data into a live, stateful door.
pub fn door_from_spawn(spawn: DoorSpawn) -> DoorRuntime {
    DoorRuntime {
        x: spawn.x,
        y: spawn.y,
        vertical: spawn.vertical,
        lock: spawn.lock,
        tile: spawn.tile,
        open_ratio: 0.0,
        state: DoorState::Closed,
        open_timer: 0.0,
    }
}

pub struct GameContext {
    pub world: World,
    pub player: Entity,
    pub move_speed: f32,
    pub turn_speed: f32,
    pub map: TileMap,
    pub doors: Vec<DoorRuntime>,
    use_requested: bool,
    fire_requested: bool,
}

impl GameContext {
    pub fn new(world: World, player: Entity, map: TileMap, doors: Vec<DoorRuntime>) -> Self {
        Self {
            world,
            player,
            move_speed: 0.0,
            turn_speed: 0.0,
            map,
            doors,
            use_requested: false,
            fire_requested: false,
        }
    }

    pub fn move_player(&mut self, amount: f32) {
        let Some(transform) = self.world.transform(self.player).copied() else {
            return;
        };

        let nx = transform.x + transform.heading.cos() * amount;
        let ny = transform.y + transform.heading.sin() * amount;

        if !self.is_point_blocked(nx, ny) {
            if let Some(player) = self.world.transform_mut(self.player) {
                player.x = nx;
                player.y = ny;
            }
        }
    }

    pub fn turn_player(&mut self, amount: f32) {
        if let Some(player) = self.world.transform_mut(self.player) {
            player.heading += amount;
        }
    }

    pub fn request_use(&mut self) {
        self.use_requested = true;
    }

    pub fn request_fire(&mut self) {
        self.fire_requested = true;
    }

    pub fn consume_use_request(&mut self) -> bool {
        let value = self.use_requested;
        self.use_requested = false;
        value
    }

    pub fn consume_fire_request(&mut self) -> bool {
        let value = self.fire_requested;
        self.fire_requested = false;
        value
    }

    pub fn player_transform(&self) -> Option<Transform> {
        self.world.transform(self.player).copied()
    }

    pub fn is_point_blocked(&self, x: f32, y: f32) -> bool {
        self.is_tile_blocked(x.floor() as i32, y.floor() as i32)
    }

    fn is_tile_blocked(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 {
            return true;
        }

        let ux = x as usize;
        let uy = y as usize;
        if ux >= self.map.width || uy >= self.map.height {
            return true;
        }

        let tile = self.map.walls[uy * self.map.width + ux];
        if tile == 0 {
            return false;
        }

        if let Some(door) = self.doors.iter().find(|door| door.x == ux && door.y == uy) {
            return door.open_ratio < 0.95;
        }

        true
    }

    /// Steps a straight-line ray for occlusion, used for enemy sight and line-of-fire checks.
    pub fn has_line_of_sight(&self, from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> bool {
        let dx = to_x - from_x;
        let dy = to_y - from_y;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance <= f32::EPSILON {
            return true;
        }

        const STEP: f32 = 0.08;
        let mut t = 0.0;
        while t < distance {
            let x = from_x + dx / distance * t;
            let y = from_y + dy / distance * t;
            if self.is_point_blocked(x, y) {
                return false;
            }
            t += STEP;
        }

        true
    }

    pub fn step_doors(&mut self, dt: f32) {
        // Timings match the original: OPENTICS=300 tics (~4.29s hold) at 70 tics/sec,
        // and doorposition sliding from 0 to 0xffff at 1024/tic (~0.91s to fully open).
        const SLIDE_RATE: f32 = 70.0 / 64.0;
        const OPEN_HOLD: f32 = 300.0 / 70.0;

        let blocker = self.player_transform();

        for door in &mut self.doors {
            match door.state {
                DoorState::Closed => {}
                DoorState::Opening => {
                    door.open_ratio = (door.open_ratio + dt * SLIDE_RATE).clamp(0.0, 1.0);
                    if door.open_ratio >= 1.0 {
                        door.state = DoorState::Open;
                        door.open_timer = OPEN_HOLD;
                    }
                }
                DoorState::Open => {
                    door.open_timer -= dt;
                    if door.open_timer <= 0.0 {
                        door.state = DoorState::Closing;
                    }
                }
                DoorState::Closing => {
                    // Don't close on the player standing in the doorway.
                    if let Some(player) = blocker {
                        if player.x.floor() as usize == door.x
                            && player.y.floor() as usize == door.y
                        {
                            door.state = DoorState::Open;
                            door.open_timer = OPEN_HOLD;
                            continue;
                        }
                    }

                    door.open_ratio = (door.open_ratio - dt * SLIDE_RATE).clamp(0.0, 1.0);
                    if door.open_ratio <= 0.0 {
                        door.state = DoorState::Closed;
                    }
                }
            }
        }
    }

    pub fn interact_door_in_front(&mut self) -> bool {
        let Some(player) = self.player_transform() else {
            return false;
        };

        let look_distance = 0.9;
        let tx = (player.x + player.heading.cos() * look_distance).floor() as i32;
        let ty = (player.y + player.heading.sin() * look_distance).floor() as i32;

        if tx < 0 || ty < 0 {
            return false;
        }

        let ux = tx as usize;
        let uy = ty as usize;

        if let Some(door) = self
            .doors
            .iter_mut()
            .find(|door| door.x == ux && door.y == uy)
        {
            if door.state == DoorState::Open || door.state == DoorState::Opening {
                door.state = DoorState::Closing;
                door.open_timer = 0.0;
            } else {
                door.state = DoorState::Opening;
            }
            return true;
        }

        false
    }
}
