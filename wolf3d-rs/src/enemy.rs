//! Enemy entities and their AI: spawning, patrol/chase state machine, and combat.
//!
//! Each enemy is a small per-entity finite state machine (Idle/Chasing/Dead)
//! layered on top of an ECS transform. Keeping this isolated from `App` means
//! enemy behavior can be tuned or tested without touching input/render code.

use macroquad::prelude::*;

use crate::data::{EnemyKind, TileMap};
use crate::ecs::{Entity, Transform, World};
use crate::renderer::Billboard;
use crate::world::GameContext;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EnemyState {
    Idle,
    Chasing,
    Dead,
}

pub struct EnemyAgent {
    pub entity: Entity,
    pub kind: EnemyKind,
    pub state: EnemyState,
    pub hp: i32,
    attack_timer: f32,
    patrol_seed: f32,
}

const MELEE_RANGE: f32 = 1.05;
const CHASE_SIGHT_RANGE: f32 = 9.0;
const ATTACK_DAMAGE: i32 = 5;
const ATTACK_COOLDOWN: f32 = 0.6;

fn starting_hp(kind: EnemyKind) -> i32 {
    match kind {
        EnemyKind::Guard => 34,
        EnemyKind::Officer => 44,
        EnemyKind::Ss => 54,
        EnemyKind::Dog => 24,
        EnemyKind::Mutant => 60,
    }
}

fn chase_speed(kind: EnemyKind) -> f32 {
    match kind {
        EnemyKind::Guard => 1.2,
        EnemyKind::Officer => 1.5,
        EnemyKind::Ss => 1.7,
        EnemyKind::Dog => 1.9,
        EnemyKind::Mutant => 1.6,
    }
}

fn billboard_style(kind: EnemyKind) -> (Color, f32) {
    match kind {
        EnemyKind::Guard => (RED, 0.72),
        EnemyKind::Officer => (ORANGE, 0.77),
        EnemyKind::Ss => (PINK, 0.80),
        EnemyKind::Dog => (BROWN, 0.58),
        EnemyKind::Mutant => (GREEN, 0.88),
    }
}

pub fn spawn_enemies(world: &mut World, map: &TileMap) -> Vec<EnemyAgent> {
    map.enemies
        .iter()
        .enumerate()
        .map(|(index, spawn)| {
            let entity = world.spawn(Transform {
                x: spawn.x,
                y: spawn.y,
                heading: if spawn.patrolling { 1.2 } else { 0.0 },
            });

            EnemyAgent {
                entity,
                kind: spawn.kind,
                state: if spawn.patrolling {
                    EnemyState::Chasing
                } else {
                    EnemyState::Idle
                },
                hp: starting_hp(spawn.kind),
                attack_timer: 0.0,
                patrol_seed: (index % 3) as f32,
            }
        })
        .collect()
}

/// Advances every living enemy's AI by `dt` and returns damage dealt to the player.
pub fn update_enemies(context: &mut GameContext, enemies: &mut [EnemyAgent], dt: f32) -> i32 {
    let Some(player) = context.player_transform() else {
        return 0;
    };

    let mut damage_to_player = 0;

    for enemy in enemies {
        if enemy.state == EnemyState::Dead {
            continue;
        }

        let Some(mut transform) = context.world.transform(enemy.entity).copied() else {
            continue;
        };

        let dx = player.x - transform.x;
        let dy = player.y - transform.y;
        let distance = (dx * dx + dy * dy).sqrt();

        enemy.attack_timer = (enemy.attack_timer - dt).max(0.0);

        if distance < CHASE_SIGHT_RANGE
            && context.has_line_of_sight(transform.x, transform.y, player.x, player.y)
        {
            enemy.state = EnemyState::Chasing;
        } else if enemy.state == EnemyState::Chasing {
            enemy.state = EnemyState::Idle;
        }

        match enemy.state {
            EnemyState::Idle => {
                if enemy.patrol_seed > 0.0 {
                    transform.heading += dt * 0.6;
                }
            }
            EnemyState::Chasing => {
                if distance > 0.7 {
                    let step = chase_speed(enemy.kind) * dt;
                    let nx = transform.x + dx / distance * step;
                    let ny = transform.y + dy / distance * step;
                    if !context.is_point_blocked(nx, ny) {
                        transform.x = nx;
                        transform.y = ny;
                    }
                }

                if distance < MELEE_RANGE && enemy.attack_timer <= 0.0 {
                    damage_to_player += ATTACK_DAMAGE;
                    enemy.attack_timer = ATTACK_COOLDOWN;
                }
            }
            EnemyState::Dead => {}
        }

        if let Some(slot) = context.world.transform_mut(enemy.entity) {
            *slot = transform;
        }
    }

    damage_to_player
}

/// Builds the sprite billboards for every living enemy, for the raycaster to draw.
pub fn billboards(context: &GameContext, enemies: &[EnemyAgent]) -> Vec<Billboard> {
    let mut output = Vec::new();

    for enemy in enemies {
        if enemy.state == EnemyState::Dead {
            continue;
        }

        let Some(transform) = context.world.transform(enemy.entity).copied() else {
            continue;
        };

        let (color, scale) = billboard_style(enemy.kind);
        output.push(Billboard {
            x: transform.x,
            y: transform.y,
            color,
            scale,
        });
    }

    output
}
