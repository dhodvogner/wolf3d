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

        // Camera-plane projection (not equal-angle steps) so straight walls stay straight
        // up close; perpendicular distance from the DDA is already fisheye-free.
        let dir_x = player.heading.cos();
        let dir_y = player.heading.sin();
        let plane_scale = (self.fov / 2.0).tan();
        let plane_x = -dir_y * plane_scale;
        let plane_y = dir_x * plane_scale;

        for col in 0..width {
            let camera_x = 2.0 * (col as f32 / width as f32) - 1.0;
            let ray_dir_x = dir_x + plane_x * camera_x;
            let ray_dir_y = dir_y + plane_y * camera_x;
            let hit = cast_ray(map, doors, player, ray_dir_x, ray_dir_y, self.max_depth);

            let safe_distance = hit.distance.max(0.001);
            depths[col as usize] = safe_distance;

            let wall_height = (height as f32 / safe_distance).min(height as f32);
            let y_top = ((height as f32 - wall_height) / 2.0).max(0.0);

            let texture = pick_wall_texture(textures, hit.wall_id, hit.hit_side);
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
    ray_dir_x: f32,
    ray_dir_y: f32,
    max_depth: f32,
) -> RayHit {
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
    let mut door_open_ratio = 0.0f32;

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
            let candidate_distance = if !hit_side {
                (map_x as f32 - player.x + (1.0 - step_x as f32) / 2.0) / ray_dir_x
            } else {
                (map_y as f32 - player.y + (1.0 - step_y as f32) / 2.0) / ray_dir_y
            }
            .abs();

            if let Some(door) = door_at(doors, map_x, map_y) {
                // A sliding door only blocks the still-closed sliver of the tile;
                // rays crossing the open portion pass through to whatever is beyond.
                let mut crossing = if !hit_side {
                    player.y + candidate_distance * ray_dir_y
                } else {
                    player.x + candidate_distance * ray_dir_x
                };
                crossing -= crossing.floor();

                let closed_extent = 1.0 - door.open_ratio;
                if crossing >= closed_extent {
                    continue;
                }

                wall_id = door.tile;
                door_open_ratio = door.open_ratio;
            } else {
                wall_id = tile;
            }

            hit_distance = candidate_distance.min(max_depth);
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

    // Stretch the still-visible sliver of a sliding door texture to fill the frame.
    wall_x = (wall_x / (1.0 - door_open_ratio).max(0.001)).clamp(0.0, 1.0);

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
    doors
        .iter()
        .copied()
        .find(|door| door.x == x && door.y == y)
}

fn pick_wall_texture(textures: &[WallTexture], wall_id: u16, hit_side: bool) -> &WallTexture {
    // Matches horizwall/vertwall (and DOORWALL) from WL_MAIN.C/WL_DRAW.C: each wall tile maps
    // to a pair of texture pages, one per hit side.
    let side_offset = if hit_side { 0 } else { 1 };

    if (90..=101).contains(&wall_id) {
        let lock = if wall_id % 2 == 0 {
            (wall_id - 90) / 2
        } else {
            (wall_id - 91) / 2
        };
        let door_offset = match lock {
            0 => 0,
            5 => 4,
            _ => 6,
        };
        let door_base = textures.len().saturating_sub(8);
        return &textures[(door_base + door_offset + side_offset) % textures.len()];
    }

    let index = wall_id.saturating_sub(1) as usize * 2 + side_offset;
    &textures[index % textures.len()]
}

fn palette_to_color(index: u8) -> Color {
    let (r, g, b) = WOLF3D_PALETTE[index as usize];
    Color::new(r as f32 / 63.0, g as f32 / 63.0, b as f32 / 63.0, 1.0)
}

// Wolfenstein 3D's compiled-in VGA palette (6-bit RGB per channel), from id_vl.cpp/wolfpal.inc.
const WOLF3D_PALETTE: [(u8, u8, u8); 256] = [
    (0, 0, 0),
    (0, 0, 42),
    (0, 42, 0),
    (0, 42, 42),
    (42, 0, 0),
    (42, 0, 42),
    (42, 21, 0),
    (42, 42, 42),
    (21, 21, 21),
    (21, 21, 63),
    (21, 63, 21),
    (21, 63, 63),
    (63, 21, 21),
    (63, 21, 63),
    (63, 63, 21),
    (63, 63, 63),
    (59, 59, 59),
    (55, 55, 55),
    (52, 52, 52),
    (48, 48, 48),
    (45, 45, 45),
    (42, 42, 42),
    (38, 38, 38),
    (35, 35, 35),
    (31, 31, 31),
    (28, 28, 28),
    (25, 25, 25),
    (21, 21, 21),
    (18, 18, 18),
    (14, 14, 14),
    (11, 11, 11),
    (8, 8, 8),
    (63, 0, 0),
    (59, 0, 0),
    (56, 0, 0),
    (53, 0, 0),
    (50, 0, 0),
    (47, 0, 0),
    (44, 0, 0),
    (41, 0, 0),
    (38, 0, 0),
    (34, 0, 0),
    (31, 0, 0),
    (28, 0, 0),
    (25, 0, 0),
    (22, 0, 0),
    (19, 0, 0),
    (16, 0, 0),
    (63, 54, 54),
    (63, 46, 46),
    (63, 39, 39),
    (63, 31, 31),
    (63, 23, 23),
    (63, 16, 16),
    (63, 8, 8),
    (63, 0, 0),
    (63, 42, 23),
    (63, 38, 16),
    (63, 34, 8),
    (63, 30, 0),
    (57, 27, 0),
    (51, 24, 0),
    (45, 21, 0),
    (39, 19, 0),
    (63, 63, 54),
    (63, 63, 46),
    (63, 63, 39),
    (63, 63, 31),
    (63, 62, 23),
    (63, 61, 16),
    (63, 61, 8),
    (63, 61, 0),
    (57, 54, 0),
    (51, 49, 0),
    (45, 43, 0),
    (39, 39, 0),
    (33, 33, 0),
    (28, 27, 0),
    (22, 21, 0),
    (16, 16, 0),
    (52, 63, 23),
    (49, 63, 16),
    (45, 63, 8),
    (40, 63, 0),
    (36, 57, 0),
    (32, 51, 0),
    (29, 45, 0),
    (24, 39, 0),
    (54, 63, 54),
    (47, 63, 46),
    (39, 63, 39),
    (32, 63, 31),
    (24, 63, 23),
    (16, 63, 16),
    (8, 63, 8),
    (0, 63, 0),
    (0, 63, 0),
    (0, 59, 0),
    (0, 56, 0),
    (0, 53, 0),
    (1, 50, 0),
    (1, 47, 0),
    (1, 44, 0),
    (1, 41, 0),
    (1, 38, 0),
    (1, 34, 0),
    (1, 31, 0),
    (1, 28, 0),
    (1, 25, 0),
    (1, 22, 0),
    (1, 19, 0),
    (1, 16, 0),
    (54, 63, 63),
    (46, 63, 63),
    (39, 63, 63),
    (31, 63, 62),
    (23, 63, 63),
    (16, 63, 63),
    (8, 63, 63),
    (0, 63, 63),
    (0, 57, 57),
    (0, 51, 51),
    (0, 45, 45),
    (0, 39, 39),
    (0, 33, 33),
    (0, 28, 28),
    (0, 22, 22),
    (0, 16, 16),
    (23, 47, 63),
    (16, 44, 63),
    (8, 42, 63),
    (0, 39, 63),
    (0, 35, 57),
    (0, 31, 51),
    (0, 27, 45),
    (0, 23, 39),
    (54, 54, 63),
    (46, 47, 63),
    (39, 39, 63),
    (31, 32, 63),
    (23, 24, 63),
    (16, 16, 63),
    (8, 9, 63),
    (0, 1, 63),
    (0, 0, 63),
    (0, 0, 59),
    (0, 0, 56),
    (0, 0, 53),
    (0, 0, 50),
    (0, 0, 47),
    (0, 0, 44),
    (0, 0, 41),
    (0, 0, 38),
    (0, 0, 34),
    (0, 0, 31),
    (0, 0, 28),
    (0, 0, 25),
    (0, 0, 22),
    (0, 0, 19),
    (0, 0, 16),
    (10, 10, 10),
    (63, 56, 13),
    (63, 53, 9),
    (63, 51, 6),
    (63, 48, 2),
    (63, 45, 0),
    (45, 8, 63),
    (42, 0, 63),
    (38, 0, 57),
    (32, 0, 51),
    (29, 0, 45),
    (24, 0, 39),
    (20, 0, 33),
    (17, 0, 28),
    (13, 0, 22),
    (10, 0, 16),
    (63, 54, 63),
    (63, 46, 63),
    (63, 39, 63),
    (63, 31, 63),
    (63, 23, 63),
    (63, 16, 63),
    (63, 8, 63),
    (63, 0, 63),
    (56, 0, 57),
    (50, 0, 51),
    (45, 0, 45),
    (39, 0, 39),
    (33, 0, 33),
    (27, 0, 28),
    (22, 0, 22),
    (16, 0, 16),
    (63, 58, 55),
    (63, 56, 52),
    (63, 54, 49),
    (63, 53, 47),
    (63, 51, 44),
    (63, 49, 41),
    (63, 47, 39),
    (63, 46, 36),
    (63, 44, 32),
    (63, 41, 28),
    (63, 39, 24),
    (60, 37, 23),
    (58, 35, 22),
    (55, 34, 21),
    (52, 32, 20),
    (50, 31, 19),
    (47, 30, 18),
    (45, 28, 17),
    (42, 26, 16),
    (40, 25, 15),
    (39, 24, 14),
    (36, 23, 13),
    (34, 22, 12),
    (32, 20, 11),
    (29, 19, 10),
    (27, 18, 9),
    (23, 16, 8),
    (21, 15, 7),
    (18, 14, 6),
    (16, 12, 6),
    (14, 11, 5),
    (10, 8, 3),
    (24, 0, 25),
    (0, 25, 25),
    (0, 24, 24),
    (0, 0, 7),
    (0, 0, 11),
    (12, 9, 4),
    (18, 0, 18),
    (20, 0, 20),
    (0, 0, 13),
    (7, 7, 7),
    (19, 19, 19),
    (23, 23, 23),
    (16, 16, 16),
    (12, 12, 12),
    (13, 13, 13),
    (54, 61, 61),
    (46, 58, 58),
    (39, 55, 55),
    (29, 50, 50),
    (18, 48, 48),
    (8, 45, 45),
    (8, 44, 44),
    (0, 41, 41),
    (0, 38, 38),
    (0, 35, 35),
    (0, 33, 33),
    (0, 31, 31),
    (0, 30, 30),
    (0, 29, 29),
    (0, 28, 28),
    (0, 27, 27),
    (38, 0, 34),
];

fn normalize_angle(mut angle: f32) -> f32 {
    while angle > std::f32::consts::PI {
        angle -= std::f32::consts::TAU;
    }
    while angle < -std::f32::consts::PI {
        angle += std::f32::consts::TAU;
    }
    angle
}
