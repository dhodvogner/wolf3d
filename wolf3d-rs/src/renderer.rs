use macroquad::prelude::*;

use crate::data::{TileMap, WallTexture};
use crate::ecs::Transform;

pub struct Raycaster {
    pub fov: f32,
    pub max_depth: f32,
}

#[derive(Clone, Copy)]
pub struct DoorRenderState {
    pub x: i32,
    pub y: i32,
    pub open_ratio: f32,
    pub tile: u16,
}

#[derive(Clone, Copy)]
pub struct Billboard {
    pub x: f32,
    pub y: f32,
    pub color: Color,
    pub scale: f32,
}

#[derive(Clone, Copy)]
struct RayHit {
    distance: f32,
    wall_id: u16,
    tex_x: f32,
    hit_side: bool,
}

impl Raycaster {
    pub fn new() -> Self {
        Self {
            fov: std::f32::consts::FRAC_PI_2,
            max_depth: 32.0,
        }
    }

    pub fn render(
        &self,
        map: &TileMap,
        doors: &[DoorRenderState],
        textures: &[WallTexture],
        player: Transform,
        billboards: &[Billboard],
    ) {
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

        if textures.is_empty() {
            return;
        }

        let mut depths = vec![self.max_depth; width.max(0) as usize];

        for col in 0..width {
            let camera = col as f32 / width as f32;
            let angle = player.heading - self.fov / 2.0 + camera * self.fov;
            let hit = cast_ray(map, doors, player, angle, self.max_depth);

            let corrected = hit.distance * (angle - player.heading).cos();
            let safe_distance = corrected.max(0.001);
            depths[col as usize] = safe_distance;

            let wall_height = (height as f32 / safe_distance).min(height as f32);
            let y_top = ((height as f32 - wall_height) / 2.0).max(0.0);

            let texture = pick_wall_texture(textures, hit.wall_id);
            if texture.texels.len() < 64 * 64 {
                continue;
            }

            let tex_x = ((hit.tex_x * 63.0).clamp(0.0, 63.0)) as usize;
            let wall_rows = wall_height.max(1.0) as i32;

            for step in 0..wall_rows {
                let screen_y = y_top as i32 + step;
                if screen_y < 0 || screen_y >= height {
                    continue;
                }

                let tex_y = ((step as f32 / wall_rows as f32) * 63.0).clamp(0.0, 63.0) as usize;
                let palette_index = texture.texels[tex_x * 64 + tex_y];
                let mut color = palette_to_color(palette_index);

                let depth_shade = 1.0 - (safe_distance / self.max_depth).min(1.0);
                let side_shade = if hit.hit_side { 0.78 } else { 1.0 };
                let shade = (0.22 + depth_shade * 0.78) * side_shade;
                color.r *= shade;
                color.g *= shade;
                color.b *= shade;

                draw_rectangle(col as f32, screen_y as f32, 1.0, 1.0, color);
            }
        }

        self.render_billboards(player, billboards, &depths, width as f32, height as f32);
    }

    fn render_billboards(
        &self,
        player: Transform,
        billboards: &[Billboard],
        depths: &[f32],
        width: f32,
        height: f32,
    ) {
        let mut indexed: Vec<(usize, f32)> = billboards
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let dx = b.x - player.x;
                let dy = b.y - player.y;
                (i, (dx * dx + dy * dy).sqrt())
            })
            .collect();

        indexed.sort_by(|a, b| b.1.total_cmp(&a.1));

        for (index, distance) in indexed {
            if distance <= 0.05 || distance > self.max_depth {
                continue;
            }

            let b = billboards[index];
            let dx = b.x - player.x;
            let dy = b.y - player.y;
            let mut angle = dy.atan2(dx) - player.heading;
            angle = normalize_angle(angle);

            if angle.abs() > self.fov * 0.7 {
                continue;
            }

            let screen_x = ((angle + self.fov / 2.0) / self.fov) * width;
            let col = screen_x as i32;
            if col < 0 || col as usize >= depths.len() {
                continue;
            }

            if distance > depths[col as usize] {
                continue;
            }

            let sprite_h = ((height / distance) * b.scale).clamp(6.0, height * 0.9);
            let sprite_w = sprite_h * 0.65;
            let x = screen_x - sprite_w / 2.0;
            let y = (height - sprite_h) / 2.0;
            draw_rectangle(x, y, sprite_w, sprite_h, b.color);
        }
    }
}

fn cast_ray(
    map: &TileMap,
    doors: &[DoorRenderState],
    player: Transform,
    angle: f32,
    max_depth: f32,
) -> RayHit {
    let ray_dir_x = angle.cos();
    let ray_dir_y = angle.sin();

    let mut map_x = player.x.floor() as i32;
    let mut map_y = player.y.floor() as i32;

    let delta_dist_x = if ray_dir_x.abs() < f32::EPSILON {
        f32::MAX
    } else {
        (1.0 / ray_dir_x).abs()
    };
    let delta_dist_y = if ray_dir_y.abs() < f32::EPSILON {
        f32::MAX
    } else {
        (1.0 / ray_dir_y).abs()
    };

    let (step_x, mut side_dist_x) = if ray_dir_x < 0.0 {
        (-1, (player.x - map_x as f32) * delta_dist_x)
    } else {
        (1, ((map_x as f32 + 1.0) - player.x) * delta_dist_x)
    };

    let (step_y, mut side_dist_y) = if ray_dir_y < 0.0 {
        (-1, (player.y - map_y as f32) * delta_dist_y)
    } else {
        (1, ((map_y as f32 + 1.0) - player.y) * delta_dist_y)
    };

    let mut wall_id = 1u16;
    let mut hit_side = false;
    let mut hit_distance = max_depth;

    for _ in 0..2048 {
        if side_dist_x < side_dist_y {
            side_dist_x += delta_dist_x;
            map_x += step_x;
            hit_side = false;
        } else {
            side_dist_y += delta_dist_y;
            map_y += step_y;
            hit_side = true;
        }

        let Some(tile) = map_tile(map, map_x, map_y) else {
            break;
        };

        if tile > 0 {
            if let Some(door) = door_at(doors, map_x, map_y) {
                if door.open_ratio >= 0.95 {
                    continue;
                }
                wall_id = door.tile;
            } else {
                wall_id = tile;
            }

            hit_distance = if !hit_side {
                (map_x as f32 - player.x + (1.0 - step_x as f32) / 2.0) / ray_dir_x
            } else {
                (map_y as f32 - player.y + (1.0 - step_y as f32) / 2.0) / ray_dir_y
            };
            hit_distance = hit_distance.abs().min(max_depth);
            break;
        }
    }

    let mut wall_x = if !hit_side {
        player.y + hit_distance * ray_dir_y
    } else {
        player.x + hit_distance * ray_dir_x
    };
    wall_x -= wall_x.floor();

    if (!hit_side && ray_dir_x > 0.0) || (hit_side && ray_dir_y < 0.0) {
        wall_x = 1.0 - wall_x;
    }

    RayHit {
        distance: hit_distance.max(0.001),
        wall_id,
        tex_x: wall_x,
        hit_side,
    }
}

fn map_tile(map: &TileMap, x: i32, y: i32) -> Option<u16> {
    if x < 0 || y < 0 {
        return None;
    }

    let ux = x as usize;
    let uy = y as usize;
    if ux >= map.width || uy >= map.height {
        return None;
    }

    Some(map.walls[uy * map.width + ux])
}

fn door_at(doors: &[DoorRenderState], x: i32, y: i32) -> Option<DoorRenderState> {
    doors.iter().copied().find(|door| door.x == x && door.y == y)
}

fn pick_wall_texture(textures: &[WallTexture], wall_id: u16) -> &WallTexture {
    let index = wall_id.saturating_sub(1) as usize;
    &textures[index % textures.len()]
}

fn palette_to_color(index: u8) -> Color {
    let r = ((index >> 5) & 0x07) as f32 / 7.0;
    let g = ((index >> 2) & 0x07) as f32 / 7.0;
    let b = (index & 0x03) as f32 / 3.0;
    Color::new(r, g, b, 1.0)
}

fn normalize_angle(mut angle: f32) -> f32 {
    while angle > std::f32::consts::PI {
        angle -= std::f32::consts::TAU;
    }
    while angle < -std::f32::consts::PI {
        angle += std::f32::consts::TAU;
    }
    angle
}
