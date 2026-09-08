use macroquad::prelude::*;

use crate::ecs::Transform;

pub struct Raycaster {
    pub fov: f32,
    pub max_depth: f32,
}

impl Raycaster {
    pub fn new() -> Self {
        Self {
            fov: std::f32::consts::FRAC_PI_2,
            max_depth: 24.0,
        }
    }

    pub fn render(&self, map: &[&str], player: Transform) {
        let width = screen_width() as i32;
        let height = screen_height() as i32;

        clear_background(BLACK);
        draw_rectangle(0.0, 0.0, width as f32, height as f32 / 2.0, SKYBLUE);
        draw_rectangle(
            0.0,
            height as f32 / 2.0,
            width as f32,
            height as f32 / 2.0,
            DARKGRAY,
        );

        for col in 0..width {
            let camera = col as f32 / width as f32;
            let angle = player.heading - self.fov / 2.0 + camera * self.fov;
            let distance = cast_ray(map, player.x, player.y, angle, self.max_depth);
            let corrected = distance * (angle - player.heading).cos();
            let safe_distance = corrected.max(0.001);
            let wall_height = (height as f32 / safe_distance).min(height as f32);

            let shade = 1.0 - (safe_distance / self.max_depth).min(1.0);
            let color = Color::new(0.2 + shade * 0.8, 0.15 + shade * 0.6, shade * 0.5, 1.0);
            draw_rectangle(
                col as f32,
                (height as f32 - wall_height) / 2.0,
                1.0,
                wall_height,
                color,
            );
        }
    }
}

fn cast_ray(map: &[&str], px: f32, py: f32, angle: f32, max_depth: f32) -> f32 {
    let step = 0.04;
    let mut depth = 0.0;
    let dx = angle.cos();
    let dy = angle.sin();

    while depth < max_depth {
        let rx = px + dx * depth;
        let ry = py + dy * depth;
        let cell_x = rx.floor() as i32;
        let cell_y = ry.floor() as i32;

        if is_wall(map, cell_x, cell_y) {
            return depth;
        }

        depth += step;
    }

    max_depth
}

fn is_wall(map: &[&str], x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || y as usize >= map.len() {
        return true;
    }

    let row = map[y as usize].as_bytes();
    if x as usize >= row.len() {
        return true;
    }

    row[x as usize] == b'#'
}
