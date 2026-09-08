mod app;
mod command;
mod data;
mod ecs;
mod events;
mod fsm;
mod pool;
mod renderer;

use app::App;

#[macroquad::main("Wolf3D Rust Modernization")]
async fn main() {
    let mut app = App::new();
    app.run().await;
}
