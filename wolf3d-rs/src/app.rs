use std::env;
use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use crate::command::{Command, MoveBackward, MoveForward, TurnLeft, TurnRight};
use crate::data::{load_assets, GameAssets, TileMap};
use crate::ecs::{Entity, Transform, World};
use crate::events::{Event, EventBus, HudEventLog};
use crate::fsm::{GameState, StateMachine};
use crate::pool::{ObjectPool, Particle};
use crate::renderer::Raycaster;

const DEFAULT_DATA_DIRS: [&str; 2] = ["./data", "."];

pub struct GameContext {
    pub world: World,
    pub player: Entity,
    pub move_speed: f32,
    pub turn_speed: f32,
    pub map: TileMap,
}

impl GameContext {
    pub fn move_player(&mut self, amount: f32) {
        let Some(transform) = self.world.transform(self.player).copied() else {
            return;
        };

        let nx = transform.x + transform.heading.cos() * amount;
        let ny = transform.y + transform.heading.sin() * amount;

        if !self.collides(nx, ny) {
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

    fn collides(&self, x: f32, y: f32) -> bool {
        let tx = x.floor() as i32;
        let ty = y.floor() as i32;

        if tx < 0 || ty < 0 {
            return true;
        }

        let ux = tx as usize;
        let uy = ty as usize;
        if ux >= self.map.width || uy >= self.map.height {
            return true;
        }

        self.map.walls[uy * self.map.width + ux] > 0
    }
}

pub struct App {
    state_machine: StateMachine,
    events: EventBus,
    context: GameContext,
    assets: GameAssets,
    renderer: Raycaster,
    particles: ObjectPool<Particle>,
    commands: Vec<Box<dyn Command>>,
    status_message: String,
}

impl App {
    pub fn new() -> Self {
        let mut events = EventBus::default();
        events.register(Box::new(HudEventLog::default()));

        let (assets, status_message) = load_or_fallback_assets();

        let mut world = World::new();
        let player = world.spawn(Transform {
            x: assets.map.player_start.x,
            y: assets.map.player_start.y,
            heading: assets.map.player_start.heading,
        });
        events.emit(Event::EntitySpawned(player));

        Self {
            state_machine: StateMachine::new(),
            events,
            context: GameContext {
                world,
                player,
                move_speed: 2.6,
                turn_speed: 2.2,
                map: assets.map.clone(),
            },
            assets,
            renderer: Raycaster::new(),
            particles: ObjectPool::with_capacity(32),
            commands: Vec::new(),
            status_message,
        }
    }

    pub async fn run(&mut self) {
        let (from, to) = self.state_machine.transition(GameState::Running);
        self.events.emit(Event::StateChanged { from, to });

        loop {
            if is_key_pressed(KeyCode::Escape) {
                match self.state_machine.state() {
                    GameState::Running => {
                        let (from, to) = self.state_machine.transition(GameState::Paused);
                        self.events.emit(Event::StateChanged { from, to });
                    }
                    GameState::Paused => {
                        let (from, to) = self.state_machine.transition(GameState::Running);
                        self.events.emit(Event::StateChanged { from, to });
                    }
                    _ => {}
                }
            }

            if is_key_pressed(KeyCode::Q) {
                let (from, to) = self.state_machine.transition(GameState::Quit);
                self.events.emit(Event::StateChanged { from, to });
            }

            if self.state_machine.state() == GameState::Quit {
                break;
            }

            if self.state_machine.state() == GameState::Running {
                self.collect_input();
                self.execute_commands();
                self.update_pool_demo();
            }

            let player = self
                .context
                .world
                .transform(self.context.player)
                .copied()
                .unwrap_or(Transform {
                    x: self.assets.map.player_start.x,
                    y: self.assets.map.player_start.y,
                    heading: self.assets.map.player_start.heading,
                });

            self.renderer
                .render(&self.context.map, &self.assets.wall_textures, player);
            self.draw_hud();
            self.events.dispatch();

            next_frame().await;
        }
    }

    fn collect_input(&mut self) {
        self.commands.clear();

        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
            self.commands.push(Box::new(MoveForward));
        }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
            self.commands.push(Box::new(MoveBackward));
        }
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
            self.commands.push(Box::new(TurnLeft));
        }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
            self.commands.push(Box::new(TurnRight));
        }
    }

    fn execute_commands(&mut self) {
        let delta = get_frame_time().clamp(0.0, 0.05);
        self.context.move_speed = 3.0 * delta;
        self.context.turn_speed = 2.3 * delta;

        for command in self.commands.drain(..) {
            command.execute(&mut self.context);
        }
    }

    fn update_pool_demo(&mut self) {
        let mut particle = self.particles.acquire();
        if let Some(player) = self.context.world.transform(self.context.player) {
            particle.x = player.x;
            particle.y = player.y;
            particle.ttl = 0.25;
        }
        self.particles.release(particle);
    }

    fn draw_hud(&self) {
        draw_text(
            "WASD/Arrows move & turn, Esc pause, Q quit",
            16.0,
            24.0,
            24.0,
            YELLOW,
        );
        draw_text(
            &format!("State: {:?}", self.state_machine.state()),
            16.0,
            48.0,
            24.0,
            WHITE,
        );
        let actors = self.context.map.info.iter().filter(|tile| **tile > 0).count();
        draw_text(
            &format!("Map: {}x{} / info tiles: {actors}", self.context.map.width, self.context.map.height),
            16.0,
            72.0,
            22.0,
            LIGHTGRAY,
        );
        draw_text(&self.status_message, 16.0, 96.0, 22.0, LIGHTGRAY);
    }
}

fn load_or_fallback_assets() -> (GameAssets, String) {
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
            Err(_) => continue,
        }
    }

    let fallback = fallback_assets();
    (
        fallback,
        "No original data found. Put MAPHEAD/GAMEMAPS/VSWAP in ./data or set WOLF3D_DATA_DIR."
            .to_string(),
    )
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
        player_start: crate::data::PlayerStart {
            x: 2.5,
            y: 2.5,
            heading: 0.0,
        },
    };

    let texture = crate::data::WallTexture {
        texels: (0..(64 * 64)).map(|i| (i % 255) as u8).collect(),
    };

    GameAssets {
        map,
        wall_textures: vec![texture],
        source_extension: "FALLBACK".to_string(),
    }
}

#[allow(dead_code)]
fn _abs_repo_path(relative: &str) -> PathBuf {
    Path::new("/home/runner/work/wolf3d/wolf3d/wolf3d-rs").join(relative)
}
