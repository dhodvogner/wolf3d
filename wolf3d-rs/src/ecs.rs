use std::collections::HashMap;

pub type Entity = u32;

#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub heading: f32,
}

#[derive(Default)]
pub struct World {
    next: Entity,
    transforms: HashMap<Entity, Transform>,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self, transform: Transform) -> Entity {
        self.next += 1;
        self.transforms.insert(self.next, transform);
        self.next
    }

    pub fn transform(&self, entity: Entity) -> Option<&Transform> {
        self.transforms.get(&entity)
    }

    pub fn transform_mut(&mut self, entity: Entity) -> Option<&mut Transform> {
        self.transforms.get_mut(&entity)
    }
}
