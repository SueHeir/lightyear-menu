//! This module contains the shared code between the client and the server.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;

use super::protocol::*;
use super::renderer::ExampleRendererPlugin;
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::utils::Duration;
use leafwing_input_manager::prelude::ActionState;
use lightyear::prelude::client::*;
use lightyear::prelude::TickManager;
use lightyear::prelude::*;

pub const SERVER_REPLICATION_INTERVAL: Duration = Duration::from_millis(20);
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 5000);
pub const MAX_VELOCITY: f32 = 500.0;
const WALL_SIZE: f32 = 350.0;
#[derive(Clone)]
pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ProtocolPlugin);
        app.add_plugins(ExampleRendererPlugin {
            show_confirmed: true,
        });
        // bundles
        app.add_systems(Startup, init);

        app.insert_resource(avian2d::sync::SyncConfig {
            transform_to_position: false,
            position_to_transform: true,
            transform_to_collider_scale: true,
        });

        // add a log at the start of the physics schedule
        app.add_systems(PhysicsSchedule, log.in_set(PhysicsStepSet::First));

        app.add_systems(FixedPostUpdate, after_physics_log);
        app.add_systems(Last, last_log);

        // registry types for reflection
        app.register_type::<PlayerId>();
    }
}

pub fn shared_config() -> SharedConfig {
    SharedConfig {
        // send an update every 100ms
        server_replication_send_interval: SERVER_REPLICATION_INTERVAL,
        ..default()
    }
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

// This system defines how we update the player's positions when we receive an input
pub(crate) fn shared_movement_behaviour(
    mut velocity: Mut<LinearVelocity>,
    action: &ActionState<PlayerActions>,
) {
    velocity.y = 0.0;
    velocity.x = 0.0;

    if action.pressed(&PlayerActions::Up) {
        velocity.y = MAX_VELOCITY;
    } else if action.pressed(&PlayerActions::Down) {
        velocity.y = -MAX_VELOCITY;
    } else {
        velocity.y = 0.0;
    }
    if action.pressed(&PlayerActions::Left) {
        velocity.x = -MAX_VELOCITY;
    } else if action.pressed(&PlayerActions::Right) {
        velocity.x = MAX_VELOCITY;
    } else {
        velocity.x = 0.0;
    }

    *velocity = LinearVelocity(velocity.clamp_length_max(MAX_VELOCITY));
}

pub(crate) fn after_physics_log(
    tick_manager: Res<TickManager>,
    rollback: Option<Res<Rollback>>,
    players: Query<
        (Entity, &Position, &Rotation),
        (Without<BallMarker>, Without<Confirmed>, With<PlayerId>),
    >,
    ball: Query<(&Position, &Rotation), (With<BallMarker>, Without<Confirmed>)>,
) {
    let tick = rollback.map_or(tick_manager.tick(), |r| {
        tick_manager.tick_or_rollback_tick(r.as_ref())
    });
    for (entity, position, rotation) in players.iter() {
        trace!(
            ?tick,
            ?entity,
            ?position,
            rotation = ?rotation.as_degrees(),
            "Player after physics update"
        );
    }
    for (position, rotation) in ball.iter() {
        trace!(?tick, ?position, ?rotation, "Ball after physics update");
    }
}

pub(crate) fn last_log(
    tick_manager: Res<TickManager>,
    players: Query<
        (
            Entity,
            &Position,
            &Rotation,
            Option<&Correction<Position>>,
            Option<&Correction<Rotation>>,
        ),
        (Without<BallMarker>, Without<Confirmed>, With<PlayerId>),
    >,
    ball: Query<(&Position, &Rotation), (With<BallMarker>, Without<Confirmed>)>,
) {
    let tick = tick_manager.tick();
    for (entity, position, rotation, correction, rotation_correction) in players.iter() {
        trace!(?tick, ?entity, ?position, ?correction, "Player LAST update");
        trace!(
            ?tick,
            ?entity,
            rotation = ?rotation.as_degrees(),
            ?rotation_correction,
            "Player LAST update"
        );
    }
    for (position, rotation) in ball.iter() {
        trace!(?tick, ?position, ?rotation, "Ball LAST update");
    }
}

pub(crate) fn log() {
    trace!("run physics schedule!");
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
                rotation_lock: LockedAxes::ROTATION_LOCKED,
            },
            wall: Wall { start, end },
            name: Name::from("wall"),
        }
    }
}
