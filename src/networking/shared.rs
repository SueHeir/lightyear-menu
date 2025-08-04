use bevy::diagnostic::LogDiagnosticsPlugin;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use core::hash::{Hash, Hasher};
use core::time::Duration;
use crossbeam_channel::{Receiver, TryRecvError};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use avian2d::prelude::*;
use leafwing_input_manager::prelude::ActionState;
use lightyear::connection::client_of::ClientOf;
use lightyear::input::input_buffer::InputBuffer;
use lightyear::prelude::*;
use tracing::Level;

use crate::networking::protocol::{
    BulletHitEvent, BulletLifetime, BulletMarker, ColorComponent, PhysicsBundle, Player,
    PlayerActions, Weapon, BULLET_SIZE, SHIP_LENGTH,
};
use crate::{GameCleanUp, GameState};

pub(crate) const MAX_VELOCITY: f32 = 200.0;
pub(crate) const WALL_SIZE: f32 = 350.0;
pub const SERVER_REPLICATION_INTERVAL: Duration = Duration::from_millis(100);
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 5000);
pub const FIXED_TIMESTEP_HZ: f64 = 64.0;

#[derive(Clone)]
pub struct SharedPlugin {
    pub(crate) show_confirmed: bool,
}

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(super::protocol::ProtocolPlugin);

        // bundles
        app.add_systems(OnEnter(GameState::Game), init);

        // Physics
        app.add_plugins(
            PhysicsPlugins::default()
                .build()
                // disable Sync as it is handled by lightyear_avian
                .disable::<SyncPlugin>(),
        );
        // .add_plugins(SyncPlugin::new(PostUpdate));

        app.insert_resource(Gravity(Vec2::ZERO));

        // our systems run in FixedUpdate, avian's systems run in FixedPostUpdate.
        app.add_systems(
            FixedUpdate,
            (process_collisions, lifetime_despawner).chain(),
        );

        app.add_event::<BulletHitEvent>();
        // registry types for reflection
        app.register_type::<Player>();
    }
}

pub(crate) fn color_from_id(client_id: PeerId) -> Color {
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
    aiq: &mut ApplyInputsQueryItem,
    tick: Tick,
) {
    let ex_force = &mut aiq.ex_force;
    let rot = &aiq.rot;
    let ang_vel = &mut aiq.ang_vel;

    const THRUSTER_POWER: f32 = 32000.;
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
}

/// NB we are not restricting this query to `Controlled` entities on the clients, because we hope to
///    receive PlayerActions for remote players ahead of the server simulating the tick (lag, input delay, etc)
///    in which case we prespawn their bullets on the correct tick, just like we do for our own bullets.
///
///    When spawning here, we add the `PreSpawned` component, and when the client receives the
///    replication packet from the server, it matches the hashes on its own `PreSpawned`, allowing it to
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
            &InputBuffer<ActionState<PlayerActions>>,
            &mut Weapon,
            Has<Controlled>,
            &Player,
        ),
        Or<(With<Predicted>, With<Replicate>)>,
    >,
    mut commands: Commands,
    timeline: Single<(&LocalTimeline, Has<Server>), Without<ClientOf>>,
) {
    if q.is_empty() {
        return;
    }

    let (timeline, is_server) = timeline.into_inner();
    let current_tick = timeline.tick();
    for (
        player_position,
        player_rotation,
        player_velocity,
        color,
        action,
        buffer,
        mut weapon,
        is_local,
        player,
    ) in q.iter_mut()
    {
        if !is_server && !is_local {
            // we only want to spawn bullets on the server, or for our own player
            // We could also pre-spawn bullets for remote players, but the problem is that if we incorrectly
            // pre-spawn a bullet (because we receive the 'Release' button too late) it would be very
            // visually distracting to temporarily see a fake bullet that then disappears.
            continue;
        }
        if !action.pressed(&PlayerActions::Fire) {
            continue;
        }

        // info!(?current_tick, player = ?player.client_id, "Buffer: {buffer}");

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
        let prespawned = PreSpawned::default_with_salt(player.client_id.to_bits());

        let bullet_entity = commands
            .spawn((
                Position(bullet_origin),
                LinearVelocity(bullet_linvel),
                ColorComponent((color.0.to_linear() * 5.0).into()), // bloom !
                BulletLifetime {
                    origin_tick: current_tick,
                    lifetime: FIXED_TIMESTEP_HZ as i16 * 2,
                },
                BulletMarker::new(player.client_id),
                PhysicsBundle::bullet(),
                prespawned,
            ))
            .id();
        debug!(
            pressed=?action.get_pressed(),
            "spawned bullet for ActionState, bullet={bullet_entity:?} ({}, {}). prev last_fire tick: {prev_last_fire_tick:?}",
            weapon.last_fire_tick.0, player.client_id
        );

        if is_server {
            commands.entity(bullet_entity).insert((
                Replicate::to_clients(NetworkTarget::All),
                PredictionTarget::to_clients(NetworkTarget::All),
            ));
        }
    }
}

// we want clients to predict the despawn due to TTL expiry, so this system runs on both client and server.
// servers despawn without replicating that fact.
pub(crate) fn lifetime_despawner(
    q: Query<(Entity, &BulletLifetime)>,
    mut commands: Commands,
    timeline: Single<(&LocalTimeline, Has<Server>), Without<ClientOf>>,
) {
    let (timeline, is_server) = timeline.into_inner();
    for (e, ttl) in q.iter() {
        if (timeline.tick() - ttl.origin_tick) > ttl.lifetime {
            commands.entity(e).prediction_despawn();
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
                external_force: ExternalForce::default(),
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
//
// Players can't collide with their own bullets.
// this is especially helpful if you are accelerating forwards while shooting, as otherwise you
// might overtake / collide on spawn with your own bullets that spawn in front of you.
pub(crate) fn process_collisions(
    collisions: Collisions,
    bullet_q: Query<(&BulletMarker, &ColorComponent, &Position)>,
    player_q: Query<&Player>,
    mut commands: Commands,
    timeline: Single<(&LocalTimeline, Has<Server>), Without<ClientOf>>,
    mut hit_ev_writer: EventWriter<BulletHitEvent>,
) {
    let (timeline, is_server) = timeline.into_inner();
    // when A and B collide, it can be reported as one of:
    // * A collides with B
    // * B collides with A
    // which is why logic is duplicated twice here
    for contacts in collisions.iter() {
        if let Ok((bullet, col, bullet_pos)) = bullet_q.get(contacts.collider1) {
            if let Ok(owner) = player_q.get(contacts.collider2) {
                if bullet.owner == owner.client_id {
                    // this is our own bullet, don't do anything
                    continue;
                }
            }
            // despawn the bullet
            commands.entity(contacts.collider1).prediction_despawn();
            let victim_client_id = player_q
                .get(contacts.collider2)
                .map_or(None, |victim_player| Some(victim_player.client_id));

            let ev = BulletHitEvent {
                bullet_owner: bullet.owner,
                victim_client_id,
                position: bullet_pos.0,
                bullet_color: col.0,
            };
            hit_ev_writer.write(ev);
        }
        if let Ok((bullet, col, bullet_pos)) = bullet_q.get(contacts.collider2) {
            if let Ok(owner) = player_q.get(contacts.collider1) {
                if bullet.owner == owner.client_id {
                    // this is our own bullet, don't do anything
                    continue;
                }
            }
            commands.entity(contacts.collider2).prediction_despawn();
            let victim_client_id = player_q
                .get(contacts.collider1)
                .map_or(None, |victim_player| Some(victim_player.client_id));

            let ev = BulletHitEvent {
                bullet_owner: bullet.owner,
                victim_client_id,
                position: bullet_pos.0,
                bullet_color: col.0,
            };
            hit_ev_writer.write(ev);
        }
    }
}

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
                events.write(msg);
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
