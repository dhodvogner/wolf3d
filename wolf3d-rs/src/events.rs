use crate::ecs::Entity;
use crate::fsm::GameState;

#[derive(Clone, Copy, Debug)]
pub enum Event {
    EntitySpawned(Entity),
    StateChanged { from: GameState, to: GameState },
    DoorUsed,
    EnemyKilled,
}

pub trait Observer {
    fn on_event(&mut self, event: &Event);
}

#[derive(Default)]
pub struct EventBus {
    observers: Vec<Box<dyn Observer>>,
    queue: Vec<Event>,
}

impl EventBus {
    pub fn register(&mut self, observer: Box<dyn Observer>) {
        self.observers.push(observer);
    }

    pub fn emit(&mut self, event: Event) {
        self.queue.push(event);
    }

    pub fn dispatch(&mut self) {
        for event in self.queue.drain(..) {
            for observer in &mut self.observers {
                observer.on_event(&event);
            }
        }
    }
}

#[derive(Default)]
pub struct HudEventLog {
    lines: Vec<String>,
}

impl Observer for HudEventLog {
    fn on_event(&mut self, event: &Event) {
        let text = match event {
            Event::EntitySpawned(entity) => format!("spawned entity {entity}"),
            Event::StateChanged { from, to } => format!("state: {from:?} -> {to:?}"),
            Event::DoorUsed => "door interaction".to_string(),
            Event::EnemyKilled => "enemy eliminated".to_string(),
        };
        self.lines.push(text);
        if self.lines.len() > 6 {
            self.lines.remove(0);
        }
    }
}
