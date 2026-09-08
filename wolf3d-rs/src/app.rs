use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use crate::command::{
    Command, FireAction, MoveBackward, MoveForward, TurnLeft, TurnRight, UseAction,
};
use crate::data::{load_assets, DoorSpawn, EnemyKind, GameAssets, TileMap};
use crate::ecs::{Entity, Transform, World};
use crate::events::{Event, EventBus, HudEventLog};
use crate::fsm::{GameState, StateMachine};
use crate::pool::{ObjectPool, Particle};
use crate::renderer::{Billboard, DoorRenderState, Raycaster};

const DEFAULT_DATA_DIRS: [&str; 2] = ["./data", "."];

#[derive(Clone, Copy, PartialEq, Eq)]
enum DoorState {
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Clone, Copy)]
pub struct DoorRuntime {
    x: usize,
    y: usize,
    vertical: bool,
    lock: u8,
    tile: u16,
    open_ratio: f32,
    state: DoorState,
    open_timer: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EnemyState {
    Idle,
    Chasing,
    Dead,
}

struct EnemyAgent {
    entity: Entity,
    kind: EnemyKind,
    state: EnemyState,
    hp: i32,
    attack_timer: f32,
    patrol_seed: f32,
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

    pub fn request_use(&mut self) {
        self.use_requested = true;
    }

    pub fn request_fire(&mut self) {
        self.fire_requested = true;
    }

    fn consume_use_request(&mut self) -> bool {
        let value = self.use_requested;
        self.use_requested = false;
        value
    }

    fn consume_fire_request(&mut self) -> bool {
        let value = self.fire_requested;
        self.fire_requested = false;
        value
    }

    fn collides(&self, x: f32, y: f32) -> bool {
        self.is_point_blocked(x, y)
    }

    fn is_point_blocked(&self, x: f32, y: f32) -> bool {
        let tx = x.floor() as i32;
        let ty = y.floor() as i32;
        self.is_tile_blocked(tx, ty)
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

    fn step_doors(&mut self, dt: f32) {
        for door in &mut self.doors {
            match door.state {
                DoorState::Closed => {}
                DoorState::Opening => {
                    door.open_ratio = (door.open_ratio + dt * 1.9).clamp(0.0, 1.0);
                    if door.open_ratio >= 0.98 {
                        door.state = DoorState::Open;
                        door.open_timer = 2.4;
                    }
                }
                DoorState::Open => {
                    door.open_timer -= dt;
                    if door.open_timer <= 0.0 {
                        door.state = DoorState::Closing;
                    }
                }
                DoorState::Closing => {
                    door.open_ratio = (door.open_ratio - dt * 1.7).clamp(0.0, 1.0);
                    if door.open_ratio <= 0.02 {
                        door.open_ratio = 0.0;
                        door.state = DoorState::Closed;
                    }
                }
            }
        }
    }

    fn interact_door_in_front(&mut self) -> bool {
        let Some(player) = self.world.transform(self.player).copied() else {
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

        if let Some(door) = self.doors.iter_mut().find(|door| door.x == ux && door.y == uy) {
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

pub struct App {
    state_machine: StateMachine,
    events: EventBus,
    context: GameContext,
    assets: GameAssets,
    renderer: Raycaster,
    particles: ObjectPool<Particle>,
    commands: Vec<Box<dyn Command>>,
    enemies: Vec<EnemyAgent>,
    player_health: i32,
    fire_cooldown: f32,
    kills: u32,
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

        let enemies = spawn_enemies(&mut world, &assets.map);
        for enemy in &enemies {
            events.emit(Event::EntitySpawned(enemy.entity));
        }

        let doors = assets
            .map
            .doors
            .iter()
            .copied()
            .map(door_from_spawn)
            .collect();

        Self {
            state_machine: StateMachine::new(),
            events,
            context: GameContext {
                world,
                player,
                move_speed: 0.0,
                turn_speed: 0.0,
                map: assets.map.clone(),
                doors,
                use_requested: false,
                fire_requested: false,
            },
            assets,
            renderer: Raycaster::new(),
            particles: ObjectPool::with_capacity(48),
            commands: Vec::new(),
            enemies,
            player_health: 100,
            fire_cooldown: 0.0,
            kills: 0,
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

            if is_key_pressed(KeyCode::Q) || self.player_health <= 0 {
                let (from, to) = self.state_machine.transition(GameState::Quit);
                self.events.emit(Event::StateChanged { from, to });
            }

            if self.state_machine.state() == GameState::Quit {
                break;
            }

            if self.state_machine.state() == GameState::Running {
                self.collect_input();
                self.execute_commands();
                self.resolve_actions();
                self.update_gameplay();
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

            let door_states = self
                .context
                .doors
                .iter()
                .map(|door| DoorRenderState {
                    x: door.x as i32,
                    y: door.y as i32,
                    open_ratio: door.open_ratio,
                    tile: door.tile,
                })
                .collect::<Vec<_>>();

            let billboards = self.enemy_billboards();

            self.renderer.render(
                &self.context.map,
                &door_states,
                &self.assets.wall_textures,
                player,
                &billboards,
            );
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
        if is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::E) {
            self.commands.push(Box::new(UseAction));
        }
        if is_key_pressed(KeyCode::LeftControl) || is_key_pressed(KeyCode::Enter) {
            self.commands.push(Box::new(FireAction));
        }
    }

    fn execute_commands(&mut self) {
        let delta = get_frame_time().clamp(0.0, 0.05);
        self.context.move_speed = 3.1 * delta;
        self.context.turn_speed = 2.4 * delta;

        for command in self.commands.drain(..) {
            command.execute(&mut self.context);
        }
    }

    fn resolve_actions(&mut self) {
        if self.context.consume_use_request() && self.context.interact_door_in_front() {
            self.events.emit(Event::DoorUsed);
        }

        if self.context.consume_fire_request() {
            self.fire_weapon();
        }
    }

    fn update_gameplay(&mut self) {
        let dt = get_frame_time().clamp(0.0, 0.05);

        self.context.step_doors(dt);
        self.update_enemies(dt);
        self.fire_cooldown = (self.fire_cooldown - dt).max(0.0);

        let mut particle = self.particles.acquire();
        if let Some(player) = self.context.world.transform(self.context.player) {
            particle.x = player.x;
            particle.y = player.y;
            particle.ttl = 0.2;
        }
        self.particles.release(particle);
    }

    fn update_enemies(&mut self, dt: f32) {
        let Some(player) = self.context.world.transform(self.context.player).copied() else {
            return;
        };

        for enemy in &mut self.enemies {
            if enemy.state == EnemyState::Dead {
                continue;
            }

            let Some(mut transform) = self.context.world.transform(enemy.entity).copied() else {
                continue;
            };

            let dx = player.x - transform.x;
            let dy = player.y - transform.y;
            let distance = (dx * dx + dy * dy).sqrt();

            enemy.attack_timer = (enemy.attack_timer - dt).max(0.0);

            if has_line_of_sight_in_context(&self.context, transform.x, transform.y, player.x, player.y) && distance < 9.0 {
                enemy.state = EnemyState::Chasing;
            } else if enemy.state == EnemyState::Chasing {
                enemy.state = EnemyState::Idle;
            }

            match enemy.state {
                EnemyState::Idle => {
                    if enemy.patrol_seed > 0.0 {
                        transform.heading += dt * 0.6;
                    }
                }
                EnemyState::Chasing => {
                    if distance > 0.7 {
                        let speed = match enemy.kind {
                            EnemyKind::Guard => 1.2,
                            EnemyKind::Officer => 1.5,
                            EnemyKind::Ss => 1.7,
                            EnemyKind::Dog => 1.9,
                            EnemyKind::Mutant => 1.6,
                        };
                        let step = speed * dt;
                        let nx = transform.x + dx / distance * step;
                        let ny = transform.y + dy / distance * step;
                        if !self.context.is_point_blocked(nx, ny) {
                            transform.x = nx;
                            transform.y = ny;
                        }
                    }

                    if distance < 1.05 && enemy.attack_timer <= 0.0 {
                        self.player_health = (self.player_health - 5).max(0);
                        enemy.attack_timer = 0.6;
                    }
                }
                EnemyState::Dead => {}
            }

            if let Some(slot) = self.context.world.transform_mut(enemy.entity) {
                *slot = transform;
            }
        }
    }

    fn fire_weapon(&mut self) {
        if self.fire_cooldown > 0.0 {
            return;
        }
        self.fire_cooldown = 0.24;

        let Some(player) = self.context.world.transform(self.context.player).copied() else {
            return;
        };

        let mut best: Option<(usize, f32)> = None;

        for (index, enemy) in self.enemies.iter().enumerate() {
            if enemy.state == EnemyState::Dead {
                continue;
            }

            let Some(target) = self.context.world.transform(enemy.entity).copied() else {
                continue;
            };

            let dx = target.x - player.x;
            let dy = target.y - player.y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > 12.0 {
                continue;
            }

            let mut delta = dy.atan2(dx) - player.heading;
            while delta > std::f32::consts::PI {
                delta -= std::f32::consts::TAU;
            }
            while delta < -std::f32::consts::PI {
                delta += std::f32::consts::TAU;
            }

            if delta.abs() > 0.13 {
                continue;
            }

            if !has_line_of_sight_in_context(&self.context, player.x, player.y, target.x, target.y) {
                continue;
            }

            match best {
                Some((_, best_distance)) if distance >= best_distance => {}
                _ => best = Some((index, distance)),
            }
        }

        if let Some((index, _)) = best {
            let enemy = &mut self.enemies[index];
            enemy.hp -= 34;
            enemy.state = EnemyState::Chasing;

            if enemy.hp <= 0 {
                enemy.state = EnemyState::Dead;
                self.kills += 1;
                self.events.emit(Event::EnemyKilled);
            }
        }
    }

    fn enemy_billboards(&self) -> Vec<Billboard> {
        let mut output = Vec::new();

        for enemy in &self.enemies {
            if enemy.state == EnemyState::Dead {
                continue;
            }

            let Some(transform) = self.context.world.transform(enemy.entity).copied() else {
                continue;
            };

            let (color, scale) = match enemy.kind {
                EnemyKind::Guard => (RED, 0.72),
                EnemyKind::Officer => (ORANGE, 0.77),
                EnemyKind::Ss => (PINK, 0.80),
                EnemyKind::Dog => (BROWN, 0.58),
                EnemyKind::Mutant => (GREEN, 0.88),
            };

            output.push(Billboard {
                x: transform.x,
                y: transform.y,
                color,
                scale,
            });
        }

        output
    }

    fn draw_hud(&self) {
        draw_text(
            "WASD/Arrows move, E/Space use door, Ctrl/Enter fire",
            16.0,
            24.0,
            22.0,
            YELLOW,
        );
        draw_text(
            &format!("State: {:?} | HP: {} | Kills: {}", self.state_machine.state(), self.player_health, self.kills),
            16.0,
            48.0,
            22.0,
            WHITE,
        );
        let enemies_alive = self
            .enemies
            .iter()
            .filter(|enemy| enemy.state != EnemyState::Dead)
            .count();
        let vertical_doors = self.context.doors.iter().filter(|door| door.vertical).count();
        let max_lock = self.context.doors.iter().map(|door| door.lock).max().unwrap_or(0);
        let info_markers = self.context.map.info.iter().filter(|tile| **tile > 0).count();
        draw_text(
            &format!(
                "Enemies: {} | Doors: {} (vertical {}) | Max lock {} | Info markers {}",
                enemies_alive,
                self.context.doors.len(),
                vertical_doors,
                max_lock,
                info_markers
            ),
            16.0,
            72.0,
            20.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!("{}", self.status_message),
            16.0,
            94.0,
            20.0,
            LIGHTGRAY,
        );
    }
}

fn has_line_of_sight_in_context(context: &GameContext, from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> bool {
    let dx = to_x - from_x;
    let dy = to_y - from_y;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance <= f32::EPSILON {
        return true;
    }

    let step = 0.08;
    let mut t = 0.0;
    while t < distance {
        let x = from_x + dx / distance * t;
        let y = from_y + dy / distance * t;
        if context.is_point_blocked(x, y) {
            return false;
        }
        t += step;
    }

    true
}

fn load_or_fallback_assets() -> (GameAssets, String) {
    let mut checked = Vec::new();
    let mut errors = Vec::new();

    for dir in data_search_order() {
        checked.push(dir.display().to_string());
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
                errors.push(format!("{} -> {error}", dir.display()));
            }
        }
    }

    let fallback = fallback_assets();
    let mut message = format!(
        "No original data found. Checked: {}. Put MAPHEAD/GAMEMAPS/VSWAP in one checked directory or set WOLF3D_DATA_DIR.",
        checked.join(", ")
    );

    if let Some(last_error) = errors.last() {
        message.push_str(" Last error: ");
        message.push_str(last_error);
    }

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

    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            dirs.push(exe_dir.join("data"));
            dirs.push(exe_dir.to_path_buf());
            if let Some(parent) = exe_dir.parent() {
                dirs.push(parent.join("data"));
                dirs.push(parent.to_path_buf());
            }
        }
    }

    dedupe_paths(dirs)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for path in paths {
        let key = path.to_string_lossy().to_string();
        if seen.insert(key) {
            result.push(path);
        }
    }

    result
}

fn spawn_enemies(world: &mut World, map: &TileMap) -> Vec<EnemyAgent> {
    map.enemies
        .iter()
        .enumerate()
        .map(|(index, spawn)| {
            let entity = world.spawn(Transform {
                x: spawn.x,
                y: spawn.y,
                heading: if spawn.patrolling { 1.2 } else { 0.0 },
            });

            let hp = match spawn.kind {
                EnemyKind::Guard => 34,
                EnemyKind::Officer => 44,
                EnemyKind::Ss => 54,
                EnemyKind::Dog => 24,
                EnemyKind::Mutant => 60,
            };

            EnemyAgent {
                entity,
                kind: spawn.kind,
                state: if spawn.patrolling {
                    EnemyState::Chasing
                } else {
                    EnemyState::Idle
                },
                hp,
                attack_timer: 0.0,
                patrol_seed: (index % 3) as f32,
            }
        })
        .collect()
}

fn door_from_spawn(spawn: DoorSpawn) -> DoorRuntime {
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
        doors: Vec::new(),
        enemies: Vec::new(),
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
