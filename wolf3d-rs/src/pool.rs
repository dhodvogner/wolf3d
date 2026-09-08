#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub ttl: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            ttl: 0.0,
        }
    }
}

pub struct ObjectPool<T: Default> {
    free: Vec<T>,
}

impl<T: Default> ObjectPool<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        let mut free = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            free.push(T::default());
        }
        Self { free }
    }

    pub fn acquire(&mut self) -> T {
        self.free.pop().unwrap_or_default()
    }

    pub fn release(&mut self, object: T) {
        self.free.push(object);
    }
}
