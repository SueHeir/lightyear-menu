//! The client plugin.
use crate::{networking::shared::apply_action_state_to_player_movement, ClientCommands, GameCleanUp, MultiplayerState};

use super::{
    protocol::{BallMarker, BulletHitEvent, BulletMarker, ColorComponent, PhysicsBundle, Player, PlayerActions},
    shared::{process_collisions, shared_config, shared_player_firing, ApplyInputsQuery},
    SteamworksResource,
};
use avian2d::prelude::{Collider, LinearVelocity, Position, Sensor};
use bevy::prelude::*;

use crossbeam_channel::Sender;
use leafwing_input_manager::prelude::{ActionState, InputMap};
use lightyear::prelude::*;
use lightyear::prelude::{client::*, server};
use lightyear::{inputs::leafwing::input_buffer::InputBuffer, prelude::*, shared::replication::components::Controlled, transport::LOCAL_SOCKET};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
    time::Duration,
};

#[derive(Resource)]
pub struct ClientCommandsSender {
    pub client_commands: Sender<ClientCommands>,
}


pub struct ExampleClientPlugin {
    pub(crate) client_commands: Sender<ClientCommands>,
}

/// Here we create the lightyear [`ClientPlugins`]
fn build_host_client_plugin() -> ClientPlugins {
    // Authentication is where you specify how the client should connect to the server
    // This is where you provide the server address.
    let net_config = NetConfig::Local { id: 0 };
    let mut config = ClientConfig {
        // part of the config needs to be shared between the client and server
        shared: shared_config(),
        net: net_config,
        ..default()
    };

    config.prediction.set_fixed_input_delay_ticks(10);
    config.prediction.correction_ticks_factor = 1.5;
    config.prediction.maximum_predicted_ticks = 100;
    ClientPlugins::new(config)
}

impl Plugin for ExampleClientPlugin {
    fn build(&self, app: &mut App) {
        // add lightyear plugins
        app.add_plugins(build_host_client_plugin());

            // all actions related-system that can be rolled back should be in FixedUpdate schedule
            app.add_systems(
            FixedUpdate,
            (
                // in host-server, we don't want to run the movement logic twice
                // disable this because we also run the movement logic in the server
                player_movement.run_if(in_state(MultiplayerState::Client)),
                // we don't spawn bullets during rollback.
                // if we have the inputs early (so not in rb) then we spawn,
                // otherwise we rely on normal server replication to spawn them
                shared_player_firing.run_if(not(is_in_rollback)).run_if(in_state(MultiplayerState::Client)),
            )
                .chain(),
        );

        app.insert_resource(ClientCommandsSender { client_commands: self.client_commands.clone()});

        app.add_systems(
            Update,
            (
                add_ball_physics.run_if(in_state(MultiplayerState::Client)),
                add_bullet_physics.run_if(in_state(MultiplayerState::Client)), // TODO better to scheduled right after replicated entities get spawned?
                handle_new_player,
            ),
        );
        app.add_systems(
            FixedUpdate,
            handle_hit_event
                .run_if(on_event::<BulletHitEvent>)
                .after(process_collisions).run_if(in_state(MultiplayerState::Client)),
        );
    }
}

pub fn setup_client(
    mut commands: Commands,
    mut client_config: ResMut<ClientConfig>,
    client_setup_info: Res<crate::ClientConfigInfo>,
    mut steamworks: ResMut<SteamworksResource>,
) {
    
    if client_setup_info.seperate_mode {
       

        client_config.prediction.set_fixed_input_delay_ticks(10);
        client_config.prediction.correction_ticks_factor = 1.5;
        client_config.prediction.maximum_predicted_ticks = 100;

        commands.connect_client();
        return
    }
    
    if client_setup_info.steam_testing {
        if let Some(id) = client_setup_info.steam_connect_to {
            let net_config = NetConfig::Steam {
                steamworks_client: Some(steamworks.steamworks.clone()),
                config: SteamConfig {
                    socket_config: SocketConfig::P2P {
                        virtual_port: 5002,
                        steam_id: id.raw(),
                    },
                    app_id: 480,
                },
                conditioner: None,
            };

            client_config.net = net_config;

            client_config.prediction.set_fixed_input_delay_ticks(10);
            client_config.prediction.correction_ticks_factor = 1.5;
            client_config.prediction.maximum_predicted_ticks = 100;

            // Connect to the server
            commands.connect_client();
        }

        return;
    }

    let v4 = Ipv4Addr::from_str(&client_setup_info.address.as_str()).unwrap();
    let port = client_setup_info.port.parse::<u16>().unwrap();

    let server_addr = SocketAddr::new(IpAddr::V4(v4), port);

    // Authentication is where you specify how the client should connect to the server
    // This is where you provide the server address.
    let auth = Authentication::Manual {
        server_addr: server_addr,
        client_id: rand::random::<u64>(),
        private_key: Key::default(),
        protocol_id: 0,
    };

    let transport;
    if client_setup_info.local_testing {
        transport = ClientTransport::UdpSocket(LOCAL_SOCKET);
    } else {
        transport = ClientTransport::UdpSocket(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            port,
        ));
    }

    // The IoConfig will specify the transport to use.
    let io = IoConfig {
        // the address specified here is the client_address, because we open a UDP socket on the client
        transport: transport,
        conditioner: Some(LinkConditionerConfig {
            incoming_latency: Duration::from_millis(50),
            incoming_jitter: Duration::from_millis(10),
            incoming_loss: 0.001,
        }),
        ..default()
    };
    // The NetConfig specifies how we establish a connection with the server.
    // We can use either Steam (in which case we will use steam sockets and there is no need to specify
    // our own io) or Netcode (in which case we need to specify our own io).
    let net_config = NetConfig::Netcode {
        auth,
        io,
        config: NetcodeConfig {
            client_timeout_secs: 10,
            ..Default::default()
        },
    };

    client_config.net = net_config;

    client_config.prediction.set_fixed_input_delay_ticks(10);
    client_config.prediction.correction_ticks_factor = 1.5;
    client_config.prediction.maximum_predicted_ticks = 100;

    // Connect to the server
    commands.connect_client();

    println!("trying to connect")
}

pub fn setup_host_client(mut commands: Commands, mut client_config: ResMut<ClientConfig>) {
    // The NetConfig specifies how we establish a connection with the server.
    // We can use either Steam (in which case we will use steam sockets and there is no need to specify
    // our own io) or Netcode (in which case we need to specify our own io).
    let net_config = NetConfig::Local { id: 0 };

    client_config.net = net_config;

    client_config.prediction.set_fixed_input_delay_ticks(6);
    client_config.prediction.correction_ticks_factor = 1.5;
    client_config.prediction.maximum_predicted_ticks = 100;
    // Connect to the server
    commands.connect_client();

    println!("trying to connect")
}



/// Blueprint pattern: when the ball gets replicated from the server, add all the components
/// that we need that are not replicated.
/// (for example physical properties that are constant, so they don't need to be networked)
///
/// We only add the physical properties on the ball that is displayed on screen (i.e the Predicted ball)
/// We want the ball to be rigid so that when players collide with it, they bounce off.
fn add_ball_physics(
    mut commands: Commands,
    mut ball_query: Query<(Entity, &BallMarker), Added<Predicted>>,
) {
    for (entity, ball) in ball_query.iter_mut() {
        info!("Adding physics to a replicated ball {entity:?}");
        commands.entity(entity).insert(ball.physics_bundle());
    }
}

/// Simliar blueprint scenario as balls, except sometimes clients prespawn bullets ahead of server
/// replication, which means they will already have the physics components.
/// So, we filter the query using `Without<Collider>`.
fn add_bullet_physics(
    mut commands: Commands,
    mut bullet_query: Query<Entity, (With<BulletMarker>, Added<Predicted>, Without<Collider>)>,
) {
    for entity in bullet_query.iter_mut() {
        info!("Adding physics to a replicated bullet:  {entity:?}");
        commands.entity(entity).insert(PhysicsBundle::bullet());
    }
}

/// Decorate newly connecting players with physics components
/// ..and if it's our own player, set up input stuff
fn handle_new_player(
    connection: Res<ClientConnection>,
    mut commands: Commands,
    mut player_query: Query<(Entity, &Player, Has<Controlled>), Added<Predicted>>,
    multiplayer_state: Res<State<MultiplayerState>>,
) {
    for (entity, player, is_controlled) in player_query.iter_mut() {
        info!("handle_new_player, entity = {entity:?} is_controlled = {is_controlled}");
        // is this our own entity?
        if is_controlled {
            info!("Own player replicated to us, adding inputmap {entity:?} {player:?}");
            commands.entity(entity).insert(InputMap::new([
                (PlayerActions::Up, KeyCode::ArrowUp),
                (PlayerActions::Down, KeyCode::ArrowDown),
                (PlayerActions::Left, KeyCode::ArrowLeft),
                (PlayerActions::Right, KeyCode::ArrowRight),
                (PlayerActions::Up, KeyCode::KeyW),
                (PlayerActions::Down, KeyCode::KeyS),
                (PlayerActions::Left, KeyCode::KeyA),
                (PlayerActions::Right, KeyCode::KeyD),
                (PlayerActions::Fire, KeyCode::Space),
            ]));
        } else {
            info!("Remote player replicated to us: {entity:?} {player:?}");
            // inserting an input buffer for other clients so that we can predict them properly
            // (the server will send other player's inputs to us; we will receive them on time thanks to input delay)
            commands
                .entity(entity)
                .insert(InputBuffer::<PlayerActions>::default());
        }

        if MultiplayerState::HostServer == *multiplayer_state.get() {
            continue;
        }
        let client_id = connection.id();
        info!(?entity, ?client_id, "adding physics to predicted player");
        commands.entity(entity).insert(PhysicsBundle::player_ship());
        

        commands.entity(entity).with_child((
            Transform::from_translation(Vec3::new(0., 10., 0.)),
            Sensor,
            Collider::capsule(8.0, 20.0),
        ));


    }
}

// Generate an explosion effect for bullet collisions
fn handle_hit_event(
    time: Res<Time>,
    mut events: EventReader<BulletHitEvent>,
    mut commands: Commands,
) {
    for ev in events.read() {
        commands.spawn((
            Transform::from_xyz(ev.position.x, ev.position.y, 0.0),
            Visibility::default(),
            crate::networking::renderer::Explosion::new(time.elapsed(), ev.bullet_color),
        ));
    }
}

// only apply movements to predicted entities
fn player_movement(
    mut q: Query<
        (
            &ActionState<PlayerActions>,
            &InputBuffer<PlayerActions>,
            ApplyInputsQuery,
        ),
        (With<Player>, With<Predicted>),
    >,
    tick_manager: Res<TickManager>,
    rollback: Option<Res<Rollback>>,
) {
    // max number of stale inputs to predict before default inputs used
    const MAX_STALE_TICKS: u16 = 6;
    // get the tick, even if during rollback
    let tick = rollback
        .as_ref()
        .map(|rb| tick_manager.tick_or_rollback_tick(rb))
        .unwrap_or(tick_manager.tick());

    for (action_state, input_buffer, mut aiq) in q.iter_mut() {
        // is the current ActionState for real?
        if input_buffer.get(tick).is_some() {
            // Got an exact input for this tick, staleness = 0, the happy path.
            apply_action_state_to_player_movement(action_state, 0, &mut aiq, tick);
            continue;
        }

        // if the true input is missing, this will be leftover from a previous tick, or the default().
        if let Some((prev_tick, prev_input)) = input_buffer.get_last_with_tick() {
            let staleness = (tick - prev_tick).max(0) as u16;
            if staleness > MAX_STALE_TICKS {
                // input too stale, apply default input (ie, nothing pressed)
                apply_action_state_to_player_movement(
                    &ActionState::default(),
                    staleness,
                    &mut aiq,
                    tick,
                );
            } else {
                // apply a stale input within our acceptable threshold.
                // we could use the staleness to decay movement forces as desired.
                apply_action_state_to_player_movement(prev_input, staleness, &mut aiq, tick);
            }
        } else {
            // no inputs in the buffer yet, can happen during initial connection.
            // apply the default input (ie, nothing pressed)
            apply_action_state_to_player_movement(action_state, 0, &mut aiq, tick);
        }
    }
}
