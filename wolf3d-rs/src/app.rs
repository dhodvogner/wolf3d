use macroquad::prelude::*;

use crate::command::{Command, MoveBackward, MoveForward, TurnLeft, TurnRight};
use crate::ecs::{Entity, Transform, World};
use crate::events::{Event, EventBus, HudEventLog};
use crate::fsm::{GameState, StateMachine};
use crate::pool::{ObjectPool, Particle};
use crate::renderer::Raycaster;

const MAP: &[&str] = &[
    "##########",
    "#........#",
    "#..##....#",
    "#........#",
    "#....#...#",
    "#........#",
    "##########",
];

pub struct GameContext {
    pub world: World,
    pub player: Entity,
    pub move_speed: f32,
    pub turn_speed: f32,
    map: &'static [&'static str],
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

        if tx < 0 || ty < 0 || ty as usize >= self.map.len() {
            return true;
        }

        let row = self.map[ty as usize].as_bytes();
        if tx as usize >= row.len() {
            return true;
        }

        row[tx as usize] == b'#'
    }
}

pub struct App {
    state_machine: StateMachine,
    events: EventBus,
    context: GameContext,
    renderer: Raycaster,
    particles: ObjectPool<Particle>,
    commands: Vec<Box<dyn Command>>,
}

impl App {
    pub fn new() -> Self {
        let mut events = EventBus::default();
        events.register(Box::new(HudEventLog::default()));

        let mut world = World::new();
        let player = world.spawn(Transform {
            x: 2.0,
            y: 2.0,
            heading: 0.0,
        });
        events.emit(Event::EntitySpawned(player));

        Self {
            state_machine: StateMachine::new(),
            events,
            context: GameContext {
                world,
                player,
                move_speed: 0.06,
                turn_speed: 0.045,
                map: MAP,
            },
            renderer: Raycaster::new(),
            particles: ObjectPool::with_capacity(16),
            commands: Vec::new(),
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
                    x: 2.0,
                    y: 2.0,
                    heading: 0.0,
                });

            self.renderer.render(MAP, player);
            self.draw_hud();
            self.events.dispatch();

            next_frame().await;
        }
    }

    fn collect_input(&mut self) {
        self.commands.clear();

        if is_key_down(KeyCode::W) {
            self.commands.push(Box::new(MoveForward));
        }
        if is_key_down(KeyCode::S) {
            self.commands.push(Box::new(MoveBackward));
        }
        if is_key_down(KeyCode::A) {
            self.commands.push(Box::new(TurnLeft));
        }
        if is_key_down(KeyCode::D) {
            self.commands.push(Box::new(TurnRight));
        }
    }

    fn execute_commands(&mut self) {
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
            "W/S move, A/D turn, Esc pause, Q quit",
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
    }
}
