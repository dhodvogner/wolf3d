//! Player weapon firing: hitscan target selection and damage application.

use crate::enemy::{EnemyAgent, EnemyState};
use crate::world::GameContext;

const FIRE_RANGE: f32 = 12.0;
const FIRE_CONE: f32 = 0.13;
const FIRE_DAMAGE: i32 = 34;

pub struct FireOutcome {
    pub kill: bool,
}

/// Picks the closest living enemy within range, cone of fire and line of sight, and damages it.
pub fn fire_weapon(context: &GameContext, enemies: &mut [EnemyAgent]) -> FireOutcome {
    let Some(player) = context.player_transform() else {
        return FireOutcome { kill: false };
    };

    let mut best: Option<(usize, f32)> = None;

    for (index, enemy) in enemies.iter().enumerate() {
        if enemy.state == EnemyState::Dead {
            continue;
        }

        let Some(target) = context.world.transform(enemy.entity).copied() else {
            continue;
        };

        let dx = target.x - player.x;
        let dy = target.y - player.y;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance > FIRE_RANGE {
            continue;
        }

        if angle_delta(dy.atan2(dx), player.heading).abs() > FIRE_CONE {
            continue;
        }

        if !context.has_line_of_sight(player.x, player.y, target.x, target.y) {
            continue;
        }

        match best {
            Some((_, best_distance)) if distance >= best_distance => {}
            _ => best = Some((index, distance)),
        }
    }

    let Some((index, _)) = best else {
        return FireOutcome { kill: false };
    };

    let enemy = &mut enemies[index];
    enemy.hp -= FIRE_DAMAGE;
    enemy.state = EnemyState::Chasing;

    let kill = enemy.hp <= 0;
    if kill {
        enemy.state = EnemyState::Dead;
    }

    FireOutcome { kill }
}

fn angle_delta(angle: f32, heading: f32) -> f32 {
    let mut delta = angle - heading;
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    delta
}
