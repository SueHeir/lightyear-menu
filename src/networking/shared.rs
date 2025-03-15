//! This module contains the shared code between the client and the server.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;

use crate::GameCleanUp;

use super::protocol::*;
use super::renderer::ExampleRendererPlugin;
use avian2d::prelude::*;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy::utils::Duration;
use crossbeam_channel::Receiver;
use crossbeam_channel::TryRecvError;
use leafwing_input_manager::prelude::ActionState;
use lightyear::prelude::client::*;
use lightyear::prelude::server::ReplicationTarget;
use lightyear::prelude::TickManager;
use lightyear::prelude::*;
use lightyear::shared::replication::components::Controlled;

pub const SERVER_REPLICATION_INTERVAL: Duration = Duration::from_millis(20);
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 5000);
pub const MAX_VELOCITY: f32 = 500.0;
const WALL_SIZE: f32 = 350.0;






#[derive(Resource)]
struct CrossbeamEventReceiver<T: Event>(Receiver<T>);

pub trait CrossbeamEventApp {
    fn add_crossbeam_event<T: Event>(&mut self, receiver: Receiver<T>) -> &mut Self;
}

impl CrossbeamEventApp for App {
    fn add_crossbeam_event<T: Event>(&mut self, receiver: Receiver<T>) -> &mut Self {
        self.insert_resource(CrossbeamEventReceiver::<T>(receiver));
        self.add_event::<T>();
        self.add_systems(PreUpdate, process_crossbeam_messages::<T>);
        self
    }
}


fn process_crossbeam_messages<T: Event>(
    receiver: Res<CrossbeamEventReceiver<T>>,
    mut events: EventWriter<T>,
) {
    loop {
        match receiver.0.try_recv() {
            Ok(msg) => {
                events.send(msg);
            }
            Err(TryRecvError::Disconnected) => {
                panic!("sender resource dropped")
            }
            Err(TryRecvError::Empty) => {
                break;
            }
        }
    }
}




#[derive(Clone)]
pub struct SharedPlugin;


impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ProtocolPlugin);
       

        // bundles
        app.add_systems(Startup, init);

        // Physics
        //
        // we use Position and Rotation as primary source of truth, so no need to sync changes
        // from Transform->Pos, just Pos->Transform.
        app.insert_resource(avian2d::sync::SyncConfig {
            transform_to_position: false,
            position_to_transform: true,
            ..default()
        });
        // We change SyncPlugin to PostUpdate, because we want the visually interpreted values
        // synced to transform every time, not just when Fixed schedule runs.
        // app.add_plugins(PhysicsPlugins::default().build());

        app.insert_resource(Gravity(Vec2::ZERO));
        // our systems run in FixedUpdate, avian's systems run in FixedPostUpdate.
        app.add_systems(
            FixedUpdate,
            (process_collisions, lifetime_despawner).chain(),
        );

        app.add_systems(PostProcessCollisions, filter_own_bullet_collisions);

        app.add_event::<BulletHitEvent>();
        // registry types for reflection
        app.register_type::<Player>();
    }
}


pub fn shared_config(visualizer: bool) -> SharedConfig {
    SharedConfig {
        // send an update every 100ms
        server_replication_send_interval: SERVER_REPLICATION_INTERVAL,
        visualizer,
        ..default()
    }
}

// Players can't collide with their own bullets.
// this is especially helpful if you are accelerating forwards while shooting, as otherwise you
// might overtake / collide on spawn with your own bullets that spawn in front of you.
fn filter_own_bullet_collisions(
    mut collisions: ResMut<Collisions>,
    q_bullets: Query<&BulletMarker>,
    q_players: Query<&Player>,
) {
    collisions.retain(|contacts| {
        if let Ok(bullet) = q_bullets.get(contacts.entity1) {
            if let Ok(player) = q_players.get(contacts.entity2) {
                if bullet.owner == player.client_id {
                    return false;
                }
            }
        }
        if let Ok(bullet) = q_bullets.get(contacts.entity2) {
            if let Ok(player) = q_players.get(contacts.entity1) {
                if bullet.owner == player.client_id {
                    return false;
                }
            }
        }
        true
    });
}

// Generate pseudo-random color from id
pub(crate) fn color_from_id(client_id: ClientId) -> Color {
    let h = (((client_id.to_bits().wrapping_mul(30)) % 360) as f32) / 360.0;
    let s = 1.0;
    let l = 0.5;
    Color::hsl(h, s, l)
}

pub(crate) fn init(mut commands: Commands) {
    commands.spawn(WallBundle::new(
        Vec2::new(-WALL_SIZE, -WALL_SIZE),
        Vec2::new(-WALL_SIZE, WALL_SIZE),
        Color::WHITE,
    ));
    commands.spawn(WallBundle::new(
        Vec2::new(-WALL_SIZE, WALL_SIZE),
        Vec2::new(WALL_SIZE, WALL_SIZE),
        Color::WHITE,
    ));
    commands.spawn(WallBundle::new(
        Vec2::new(WALL_SIZE, WALL_SIZE),
        Vec2::new(WALL_SIZE, -WALL_SIZE),
        Color::WHITE,
    ));
    commands.spawn(WallBundle::new(
        Vec2::new(WALL_SIZE, -WALL_SIZE),
        Vec2::new(-WALL_SIZE, -WALL_SIZE),
        Color::WHITE,
    ));
}

// #[derive(QueryData)]
// #[query_data(mutable, derive(Debug))]
// pub struct ApplyInputsQuery {
//     pub lin_vel: &'static mut LinearVelocity,
//     pub player: &'static Player,
// }

#[derive(QueryData)]
#[query_data(mutable, derive(Debug))]
pub struct ApplyInputsQuery {
    pub ex_force: &'static mut ExternalForce,
    pub ang_vel: &'static mut AngularVelocity,
    pub rot: &'static Rotation,
    pub player: &'static Player,
}

/// applies forces based on action state inputs
pub fn apply_action_state_to_player_movement(
    action: &ActionState<PlayerActions>,
    staleness: u16,
    aiq: &mut ApplyInputsQueryItem,
    tick: Tick,
) {
     // #[cfg(target_family = "wasm")]
    // if !action.get_pressed().is_empty() {
    //     info!(
    //         "{} {:?} {tick:?} = {:?} staleness = {staleness}",
    //         if staleness > 0 { "🎹😐" } else { "🎹" },
    //         aiq.player.client_id,
    //         action.get_pressed(),
    //     );
    // }

    let ex_force = &mut aiq.ex_force;
    let rot = &aiq.rot;
    let ang_vel = &mut aiq.ang_vel;

    const THRUSTER_POWER: f32 = 32.;
    const ROTATIONAL_SPEED: f32 = 4.0;

    if action.pressed(&PlayerActions::Up) {
        ex_force
            .apply_force(*rot * (Vec2::Y * THRUSTER_POWER))
            .with_persistence(false);
    }
    let desired_ang_vel = if action.pressed(&PlayerActions::Left) {
        ROTATIONAL_SPEED
    } else if action.pressed(&PlayerActions::Right) {
        -ROTATIONAL_SPEED
    } else {
        0.0
    };
    if ang_vel.0 != desired_ang_vel {
        ang_vel.0 = desired_ang_vel;
    }

    // let lin_vel = &mut aiq.lin_vel;


    // let mut move_dir = Vec2::ZERO;

    // if action.pressed(&PlayerActions::Up) {
    //     move_dir.y += 1.0;
    // }
    // if action.pressed(&PlayerActions::Down) {
    //     move_dir.y -= 1.0;
    // }
    // if action.pressed(&PlayerActions::Left) {
    //     move_dir.x -= 1.0;
    // }
    // if action.pressed(&PlayerActions::Right) {
    //     move_dir.x += 1.0;
    // }

    // move_dir = move_dir.normalize();

    // lin_vel.x = move_dir.x * 100.0;
    // lin_vel.y = move_dir.y * 100.0;

}

/// NB we are not restricting this query to `Controlled` entities on the clients, because we hope to
///    receive PlayerActions for remote players ahead of the server simulating the tick (lag, input delay, etc)
///    in which case we prespawn their bullets on the correct tick, just like we do for our own bullets.
///
///    When spawning here, we add the `PreSpawnedPlayerObject` component, and when the client receives the
///    replication packet from the server, it matches the hashes on its own `PreSpawnedPlayerObject`, allowing it to
///    treat our locally spawned one as the `Predicted` entity (and gives it the Predicted component).
///
///    This system doesn't run in rollback, so without early player inputs, their bullets will be
///    spawned by the normal server replication (triggering a rollback).
pub fn shared_player_firing(
    mut q: Query<
        (
            &Position,
            &Rotation,
            &LinearVelocity,
            &ColorComponent,
            &ActionState<PlayerActions>,
            &mut Weapon,
            Has<Controlled>,
            &Player,
        ),
        Or<(With<Predicted>, With<ReplicationTarget>)>,
    >,
    mut commands: Commands,
    tick_manager: Res<TickManager>,
    identity: NetworkIdentity,
) {
    if q.is_empty() {
        return;
    }

    let current_tick = tick_manager.tick();
    for (
        player_position,
        player_rotation,
        player_velocity,
        color,
        action,
        mut weapon,
        is_local,
        player,
    ) in q.iter_mut()
    {
        if !action.pressed(&PlayerActions::Fire) {
            continue;
        }
        let wrapped_diff = weapon.last_fire_tick - current_tick;
        if wrapped_diff.abs() <= weapon.cooldown as i16 {
            // cooldown period - can't fire.
            if weapon.last_fire_tick == current_tick {
                // logging because debugging latency edge conditions where
                // inputs arrive on exact frame server replicates to you.
                info!("Can't fire, fired this tick already! {current_tick:?}");
            } else {
                // info!("cooldown. {weapon:?} current_tick = {current_tick:?} wrapped_diff: {wrapped_diff}");
            }
            continue;
        }
        let prev_last_fire_tick = weapon.last_fire_tick;
        weapon.last_fire_tick = current_tick;

        // bullet spawns just in front of the nose of the ship, in the direction the ship is facing,
        // and inherits the speed of the ship.
        let bullet_spawn_offset = Vec2::Y * (2.0 + (SHIP_LENGTH + BULLET_SIZE) / 2.0);

        let bullet_origin = player_position.0 + player_rotation * bullet_spawn_offset;
        let bullet_linvel = player_rotation * (Vec2::Y * weapon.bullet_speed) + player_velocity.0;

        // the default hashing algorithm uses the tick and component list. in order to disambiguate
        // between two players spawning a bullet on the same tick, we add client_id to the mix.
        let prespawned = PreSpawnedPlayerObject::default_with_salt(player.client_id.to_bits());

        let bullet_entity = commands
            .spawn((
                BulletBundle::new(
                    player.client_id,
                    bullet_origin,
                    bullet_linvel,
                    (color.0.to_linear() * 5.0).into(), // bloom!
                    current_tick,
                ),
                PhysicsBundle::bullet(),
                prespawned,
            ))
            .id();
        debug!(
            "spawned bullet for ActionState, bullet={bullet_entity:?} ({}, {}). prev last_fire tick: {prev_last_fire_tick:?}",
            weapon.last_fire_tick.0, player.client_id
        );

        if identity.is_server() {
            let replicate = server::Replicate {
                sync: server::SyncTarget {
                    prediction: NetworkTarget::All,
                    ..Default::default()
                },
                // make sure that all entities that are predicted are part of the same replication group
                group: REPLICATION_GROUP,
                ..default()
            };
            commands.entity(bullet_entity).insert(replicate);
        }
    }
}

// we want clients to predict the despawn due to TTL expiry, so this system runs on both client and server.
// servers despawn without replicating that fact.
pub(crate) fn lifetime_despawner(
    q: Query<(Entity, &Lifetime)>,
    mut commands: Commands,
    tick_manager: Res<TickManager>,
    identity: NetworkIdentity,
) {
    for (e, ttl) in q.iter() {
        if (tick_manager.tick() - ttl.origin_tick) > ttl.lifetime {
            // if ttl.origin_tick.wrapping_add(ttl.lifetime) > *tick_manager.tick() {
            if identity.is_server() {
                // info!("Despawning {e:?} without replication");
                commands.entity(e).despawn();
            } else {
                // info!("Despawning:lifetime {e:?}");
                commands.entity(e).prediction_despawn();
            }
        }
    }
}

// Wall
#[derive(Bundle)]
pub(crate) struct WallBundle {
    color: ColorComponent,
    physics: PhysicsBundle,
    wall: Wall,
    name: Name,
}

#[derive(Component)]
pub(crate) struct Wall {
    pub(crate) start: Vec2,
    pub(crate) end: Vec2,
}

impl WallBundle {
    pub(crate) fn new(start: Vec2, end: Vec2, color: Color) -> Self {
        Self {
            color: ColorComponent(color),
            physics: PhysicsBundle {
                collider: Collider::segment(start, end),
                collider_density: ColliderDensity(1.0),
                rigid_body: RigidBody::Static,
                locked_axis: LockedAxes::new(),
                game_clean_up: GameCleanUp,
            },
            wall: Wall { start, end },
            name: Name::new("Wall"),
            
        }
    }
}

// Despawn bullets that collide with something.
//
// Generate a BulletHitEvent so we can modify scores, show visual effects, etc.
pub(crate) fn process_collisions(
    mut collision_event_reader: EventReader<Collision>,
    bullet_q: Query<(&BulletMarker, &ColorComponent, &Position)>,
    player_q: Query<&Player>,
    mut commands: Commands,
    tick_manager: Res<TickManager>,
    identity: NetworkIdentity,
    mut hit_ev_writer: EventWriter<BulletHitEvent>,
) {
    // when A and B collide, it can be reported as one of:
    // * A collides with B
    // * B collides with A
    // which is why logic is duplicated twice here
    for Collision(contacts) in collision_event_reader.read() {
        if let Ok((bullet, col, bullet_pos)) = bullet_q.get(contacts.entity1) {
            // despawn the bullet
            if identity.is_server() {
                commands.entity(contacts.entity1).despawn();
            } else {
                commands.entity(contacts.entity1).prediction_despawn();
            }
            let victim_client_id = player_q
                .get(contacts.entity2)
                .map_or(None, |victim_player| Some(victim_player.client_id));

            let ev = BulletHitEvent {
                bullet_owner: bullet.owner,
                victim_client_id,
                position: bullet_pos.0,
                bullet_color: col.0,
            };
            hit_ev_writer.send(ev);
        }
        if let Ok((bullet, col, bullet_pos)) = bullet_q.get(contacts.entity2) {
            if identity.is_server() {
                commands.entity(contacts.entity2).despawn();
            } else {
                commands.entity(contacts.entity2).prediction_despawn();
            }
            let victim_client_id = player_q
                .get(contacts.entity1)
                .map_or(None, |victim_player| Some(victim_player.client_id));

            let ev = BulletHitEvent {
                bullet_owner: bullet.owner,
                victim_client_id,
                position: bullet_pos.0,
                bullet_color: col.0,
            };
            hit_ev_writer.send(ev);
        }
    }
}
