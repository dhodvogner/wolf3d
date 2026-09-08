#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    Boot,
    Running,
    Paused,
    Quit,
}

pub struct StateMachine {
    state: GameState,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            state: GameState::Boot,
        }
    }

    pub fn state(&self) -> GameState {
        self.state
    }

    pub fn transition(&mut self, next: GameState) -> (GameState, GameState) {
        let previous = self.state;
        self.state = next;
        (previous, next)
    }
}
