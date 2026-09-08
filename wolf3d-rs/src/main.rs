mod app;
mod assets;
mod combat;
mod command;
mod data;
mod ecs;
mod enemy;
mod events;
mod fsm;
mod hud;
mod pool;
mod renderer;
mod world;

use app::App;

#[macroquad::main("Wolf3D Rust Modernization")]
async fn main() {
    let mut app = App::new();
    app.run().await;
}
