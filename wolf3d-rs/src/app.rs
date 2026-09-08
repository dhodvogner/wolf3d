//! Application entry point and game loop orchestration.
//!
//! `App` wires together the subsystems (input → commands → world → AI →
//! rendering) but contains no simulation logic itself: collision/doors live
//! in [`crate::world`], enemy AI in [`crate::enemy`], firing in
//! [`crate::combat`], HUD text in [`crate::hud`], and asset loading in
//! [`crate::assets`].

use macroquad::prelude::*;

use crate::assets::load_or_fallback_assets;
use crate::combat::fire_weapon;
use crate::command::{
    Command, FireAction, MoveBackward, MoveForward, TurnLeft, TurnRight, UseAction,
};
use crate::data::GameAssets;
use crate::ecs::{Entity, Transform, World};
use crate::enemy::{self, EnemyAgent, EnemyState};
use crate::events::{Event, EventBus, HudEventLog};
use crate::fsm::{GameState, StateMachine};
use crate::hud::{self, HudStats};
use crate::pool::{ObjectPool, Particle};
use crate::renderer::{DoorRenderState, Raycaster};
use crate::world::{GameContext, door_from_spawn};

const MOVE_SPEED: f32 = 3.1;
const TURN_SPEED: f32 = 2.4;
const MAX_FRAME_DT: f32 = 0.05;
const FIRE_COOLDOWN: f32 = 0.24;
const PLAYER_MAX_HEALTH: i32 = 100;

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
        let player = spawn_player(&mut world, &assets);
        events.emit(Event::EntitySpawned(player));

        let enemies = enemy::spawn_enemies(&mut world, &assets.map);
        for agent in &enemies {
            events.emit(Event::EntitySpawned(agent.entity));
        }

        let doors = assets
            .map
            .doors
            .iter()
            .copied()
            .map(door_from_spawn)
            .collect();

        let context = GameContext::new(world, player, assets.map.clone(), doors);

        Self {
            state_machine: StateMachine::new(),
            events,
            context,
            assets,
            renderer: Raycaster::new(),
            particles: ObjectPool::with_capacity(48),
            commands: Vec::new(),
            enemies,
            player_health: PLAYER_MAX_HEALTH,
            fire_cooldown: 0.0,
            kills: 0,
            status_message,
        }
    }

    pub async fn run(&mut self) {
        self.transition_state(GameState::Running);

        loop {
            self.handle_state_transitions();

            if self.state_machine.state() == GameState::Quit {
                break;
            }

            if self.state_machine.state() == GameState::Running {
                self.collect_input();
                self.execute_commands();
                self.resolve_actions();
                self.update_gameplay();
            }

            self.render_frame();
            self.events.dispatch();

            next_frame().await;
        }
    }

    fn transition_state(&mut self, next: GameState) {
        let (from, to) = self.state_machine.transition(next);
        self.events.emit(Event::StateChanged { from, to });
    }

    fn handle_state_transitions(&mut self) {
        if is_key_pressed(KeyCode::Escape) {
            match self.state_machine.state() {
                GameState::Running => self.transition_state(GameState::Paused),
                GameState::Paused => self.transition_state(GameState::Running),
                _ => {}
            }
        }

        if is_key_pressed(KeyCode::Q) || self.player_health <= 0 {
            self.transition_state(GameState::Quit);
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
        let delta = frame_delta();
        self.context.move_speed = MOVE_SPEED * delta;
        self.context.turn_speed = TURN_SPEED * delta;

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
        let dt = frame_delta();

        self.context.step_doors(dt);
        let damage = enemy::update_enemies(&mut self.context, &mut self.enemies, dt);
        self.player_health = (self.player_health - damage).max(0);
        self.fire_cooldown = (self.fire_cooldown - dt).max(0.0);

        self.spawn_ambient_particle();
    }

    fn spawn_ambient_particle(&mut self) {
        let mut particle = self.particles.acquire();
        if let Some(player) = self.context.player_transform() {
            particle.x = player.x;
            particle.y = player.y;
            particle.ttl = 0.2;
        }
        self.particles.release(particle);
    }

    fn fire_weapon(&mut self) {
        if self.fire_cooldown > 0.0 {
            return;
        }
        self.fire_cooldown = FIRE_COOLDOWN;

        let outcome = fire_weapon(&self.context, &mut self.enemies);
        if outcome.kill {
            self.kills += 1;
            self.events.emit(Event::EnemyKilled);
        }
    }

    fn render_frame(&self) {
        let player = self.context.player_transform().unwrap_or(Transform {
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

        let billboards = enemy::billboards(&self.context, &self.enemies);

        self.renderer.render(
            &self.context.map,
            &door_states,
            &self.assets.wall_textures,
            player,
            &billboards,
        );

        let enemies_alive = self
            .enemies
            .iter()
            .filter(|enemy| enemy.state != EnemyState::Dead)
            .count();

        hud::draw(
            &self.context,
            &HudStats {
                state: self.state_machine.state(),
                player_health: self.player_health,
                kills: self.kills,
                enemies_alive,
                status_message: &self.status_message,
            },
        );
    }
}

fn frame_delta() -> f32 {
    get_frame_time().clamp(0.0, MAX_FRAME_DT)
}

fn spawn_player(world: &mut World, assets: &GameAssets) -> Entity {
    world.spawn(Transform {
        x: assets.map.player_start.x,
        y: assets.map.player_start.y,
        heading: assets.map.player_start.heading,
    })
}
