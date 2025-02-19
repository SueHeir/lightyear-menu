//! This module contains the shared code between the client and the server.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;

use avian2d::math::AdjustPrecision;
use avian2d::math::Scalar;
use avian2d::prelude::*;
use bevy::ecs::entity::Entities;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy::utils::Duration;

use leafwing_input_manager::prelude::ActionState;
use lightyear::prelude::server::ReplicationTarget;
use bevy::prelude::TransformSystem::TransformPropagate;
use lightyear::shared::replication::components::Controlled;

use lightyear::prelude::client::*;
use lightyear::prelude::TickManager;
use lightyear::prelude::*;



use super::protocol::*;
pub const FIXED_TIMESTEP_HZ: f64 = 64.0;
pub const SERVER_REPLICATION_INTERVAL: Duration = Duration::from_millis(20);
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 5000);

#[derive(Clone)]
pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ProtocolPlugin);
        // bundles
        app.add_systems(Startup, init);

        app.insert_resource(avian2d::sync::SyncConfig {
            transform_to_position: false,
            position_to_transform: true,
            transform_to_collider_scale: true,
        });
        app.add_systems(
            PostProcessCollisions,
            (
                kinematic_controller_collisions,
            )
                .chain(),
        );

        // add a log at the start of the physics schedule
        app.add_systems(PhysicsSchedule, log.in_set(PhysicsStepSet::First));

        app.add_systems(
            FixedUpdate,
            (lifetime_despawner).chain(),
        );

        // registry types for reflection
        app.register_type::<PlayerNetworkInfo>();


        // set up visual interp plugins for Transform
        app.add_plugins(VisualInterpolationPlugin::<Transform>::default());

        // // observers that add VisualInterpolationStatus components to entities which receive
        // // a Position
        app.add_observer(add_visual_interpolation_components);
    }
}

pub(crate) fn init(mut _commands: Commands) {}

#[derive(QueryData)]
#[query_data(mutable, derive(Debug))]
pub struct ApplyInputsQuery {
    pub velocity: &'static mut LinearVelocity,
    pub position: &'static mut Position,
}

// This system defines how we update the player's positions when we receive an input
pub(crate) fn shared_movement_behaviour<'a>(
    mut velocity: Mut<'a, LinearVelocity>,
    action: &'a ActionState<PlayerActions>,
) {

    velocity.y = 0.0;
    velocity.x = 0.0;

    // Handle moving.
    let move_dir = action.axis_pair(&PlayerActions::Move).clamp_length_max(1.0);

    if move_dir.x != 0.0 {
        velocity.x = -move_dir.x * 150.0;
    }
    if move_dir.y != 0.0 {
        velocity.y = -move_dir.y * 150.0;
    }

    // *velocity = LinearVelocity(velocity.clamp_length_max(MAX_VELOCITY));

    // println!("{}", mouse_pos);
}

// // Non-wall entities get some visual interpolation by adding the lightyear
// // VisualInterpolateStatus component
// //
// // We query filter With<Predicted> so that the correct client entities get visual-interpolation.
// // We don't want to visually interpolate the client's Confirmed entities, since they are not rendered.
// //
// // We must trigger change detection so that the Transform updates from interpolation
// // will be propagated to children (sprites, meshes, text, etc.)
fn add_visual_interpolation_components(
    // We use Position because it's added by avian later, and when it's added
    // we know that Predicted is already present on the entity
    trigger: Trigger<OnAdd, Position>,
    q: Query<Entity, (With<Predicted>)>,
    mut commands: Commands,
) {
    if !q.contains(trigger.entity()) {
        return;
    }
    debug!("Adding visual interp component to {:?}", trigger.entity());
    commands
        .entity(trigger.entity())
        .insert(VisualInterpolateStatus::<Transform> {
            // We must trigger change detection on visual interpolation
            // to make sure that child entities (sprites, meshes, text)
            // are also interpolated
            trigger_change_detection: true,
            ..default()
        });
}


pub(crate) fn log() {
    trace!("run physics schedule!");
}

pub fn shared_config() -> SharedConfig {
    SharedConfig {
        // send an update every 100ms
        server_replication_send_interval: SERVER_REPLICATION_INTERVAL,
        ..default()
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
            if identity.is_server()|| identity.is_host_server() {
                // info!("Despawning {e:?} without replication");
                commands.entity(e).despawn_recursive();
            } else {
                // info!("Despawning:lifetime {e:?}");
                commands.entity(e).prediction_despawn();
            }
        }
    }
}

/// Kinematic bodies do not get pushed by collisions by default,
/// so it needs to be done manually.
///
/// This system handles collision response for kinematic character controllers
/// by pushing them along their contact normals by the current penetration depth,
/// and applying velocity corrections in order to snap to slopes, slide along walls,
/// and predict collisions using speculative contacts.
#[allow(clippy::type_complexity)]
fn kinematic_controller_collisions(
    collisions: Res<Collisions>,
    bodies: Query<&RigidBody>,
    collider_parents: Query<&ColliderParent, Without<Sensor>>,
    mut character_controllers: Query<
        (&mut Position, &Rotation, &mut LinearVelocity),
        (With<RigidBody>, With<PlayerNetworkInfo>),
    >,
    time: Res<Time>,
) {
    // Iterate through collisions and move the kinematic body to resolve penetration
    for contacts in collisions.iter() {
        // Get the rigid body entities of the colliders (colliders could be children)
        let Ok([collider_parent1, collider_parent2]) =
            collider_parents.get_many([contacts.entity1, contacts.entity2])
        else {
            continue;
        };

        // Get the body of the character controller and whether it is the first
        // or second entity in the collision.
        let is_first: bool;

        let character_rb: RigidBody;
        let is_other_dynamic: bool;
        let both_kinematic: bool;

        let (mut position, rotation, mut linear_velocity) =
            if let Ok(character) = character_controllers.get_mut(collider_parent1.get()) {
                is_first = true;
                character_rb = *bodies.get(collider_parent1.get()).unwrap();
                is_other_dynamic = bodies
                    .get(collider_parent2.get())
                    .is_ok_and(|rb| rb.is_dynamic());
                both_kinematic = bodies
                    .get(collider_parent2.get())
                    .is_ok_and(|rb| rb.is_kinematic());

                character
            } else if let Ok(character) = character_controllers.get_mut(collider_parent2.get()) {
                is_first = false;
                character_rb = *bodies.get(collider_parent2.get()).unwrap();
                is_other_dynamic = bodies
                    .get(collider_parent1.get())
                    .is_ok_and(|rb| rb.is_dynamic());
                both_kinematic = bodies
                    .get(collider_parent1.get())
                    .is_ok_and(|rb| rb.is_kinematic());
                character
            } else {
                continue;
            };

        // This system only handles collision response for kinematic character controllers.
        if !character_rb.is_kinematic() {
            continue;
        }

        if both_kinematic {
            continue;
        }

        // Iterate through contact manifolds and their contacts.
        // Each contact in a single manifold shares the same contact normal.
        for manifold in contacts.manifolds.iter() {
            let normal = if is_first {
                -manifold.global_normal1(rotation)
            } else {
                -manifold.global_normal2(rotation)
            };

            let mut deepest_penetration: Scalar = Scalar::MIN;

            // Solve each penetrating contact in the manifold.
            for contact in manifold.contacts.iter() {
                if contact.penetration > 0.0 {
                    position.0 += normal * contact.penetration;
                }
                deepest_penetration = deepest_penetration.max(contact.penetration);
            }

            // For now, this system only handles velocity corrections for collisions against static geometry.
            if is_other_dynamic {
                continue;
            }
            // Player Touches another player
            if both_kinematic {
                continue;
                // // Don't apply an impulse if the character is moving away from the surface.
                // if linear_velocity.dot(normal) > 0.0 {
                //     continue;
                // }

                // // Slide along the surface, rejecting the velocity along the contact normal.
                // let impulse = linear_velocity.reject_from_normalized(normal);
                // linear_velocity.0 = impulse;

                // if is_first {

                //         character.2.0 = -impulse;

                // } else {

                // }
            }

            if deepest_penetration > 0.0 {
                // If the slope is climbable, snap the velocity so that the character
                // up and down the surface smoothly.

                // The character is intersecting an unclimbable object, like a wall.
                // We want the character to slide along the surface, similarly to
                // a collide-and-slide algorithm.

                // Don't apply an impulse if the character is moving away from the surface.
                if linear_velocity.dot(normal) > 0.0 {
                    continue;
                }

                // Slide along the surface, rejecting the velocity along the contact normal.
                let impulse = linear_velocity.reject_from_normalized(normal);
                linear_velocity.0 = impulse;
            } else {
                // The character is not yet intersecting the other object,
                // but the narrow phase detected a speculative collision.
                //
                // We need to push back the part of the velocity
                // that would cause penetration within the next frame.

                let normal_speed = linear_velocity.dot(normal);

                // Don't apply an impulse if the character is moving away from the surface.
                if normal_speed > 0.0 {
                    continue;
                }

                // Compute the impulse to apply.
                let impulse_magnitude =
                    normal_speed - (deepest_penetration / time.delta_secs_f64().adjust_precision());
                let impulse = impulse_magnitude * normal;

                // // Avoid climbing up walls.
                // impulse.y = impulse.y.max(0.0);
                // impulse.x = impulse.x.max(0.0);
                linear_velocity.0 -= impulse;
            }
        }
    }
}
