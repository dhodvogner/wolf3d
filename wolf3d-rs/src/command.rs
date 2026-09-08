use crate::app::GameContext;

pub trait Command {
    fn execute(&self, context: &mut GameContext);
}

pub struct MoveForward;
pub struct MoveBackward;
pub struct TurnLeft;
pub struct TurnRight;

impl Command for MoveForward {
    fn execute(&self, context: &mut GameContext) {
        context.move_player(context.move_speed);
    }
}

impl Command for MoveBackward {
    fn execute(&self, context: &mut GameContext) {
        context.move_player(-context.move_speed);
    }
}

impl Command for TurnLeft {
    fn execute(&self, context: &mut GameContext) {
        context.turn_player(-context.turn_speed);
    }
}

impl Command for TurnRight {
    fn execute(&self, context: &mut GameContext) {
        context.turn_player(context.turn_speed);
    }
}
