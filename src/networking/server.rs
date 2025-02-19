//! The server side of the example.
//! It is possible (and recommended) to run the server in headless mode (without any rendering plugins).
//!
//! The server will:
//! - spawn a new player entity for each client that connects
//! - read inputs from the clients and move the player entities accordingly
//!
//! Lightyear will handle the replication of entities automatically if you add a `Replicate` component to them.
use crate::networking::shared::shared_movement_behaviour;
use crate::{GameCleanUp, MultiplayerState};
use bevy::prelude::*;
use lightyear::connection::server::{ConnectionRequestHandler, DeniedReason};
use lightyear::inputs::leafwing::{input_buffer::InputBuffer, input_message::InputTarget};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use lightyear::server::input::leafwing::InputSystemSet;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc};
use std::time::Duration;

use super::protocol::*;
use avian2d::prelude::*;

use leafwing_input_manager::prelude::*;
use lightyear::shared::replication::components::Controlled;

use super::shared::{
    shared_config, ApplyInputsQuery, SERVER_ADDR, SERVER_REPLICATION_INTERVAL,
};

pub struct ExampleServerPlugin {
    pub(crate) predict_all: bool,
    pub steam_client: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>,
}

/// Here we create the lightyear [`ServerPlugins`]
fn build_server_plugin(steam_client: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>) -> ServerPlugins{
    // The IoConfig will specify the transport to use.
    let io = IoConfig {
        // the address specified here is the server_address, because we open a UDP socket on the server
        transport: ServerTransport::UdpSocket(SERVER_ADDR),
        conditioner: Some(LinkConditionerConfig { incoming_latency: Duration::from_millis(80), incoming_jitter:  Duration::from_millis(10), incoming_loss: 0.001 }),
        ..default()
    };
    // The NetConfig specifies how we establish a connection with the server.
    // We can use either Steam (in which case we will use steam sockets and there is no need to specify
    // our own io) or Netcode (in which case we need to specify our own io).
    let net_config = NetConfig::Netcode {
        io,
        config: NetcodeConfig::default(),
    };

    let steam_config = NetConfig::Steam { 
        steamworks_client: Some(steam_client.clone()), 
        config: SteamConfig { 
            app_id: 480, 
            socket_config: SocketConfig::P2P { virtual_port: 5002 },//SocketConfig::Ip { server_ip: Ipv4Addr::UNSPECIFIED, game_port: 5003, query_port: 27016 }, 
            max_clients: 10, 
            connection_request_handler: Arc::new(GnomellaConnectionRequestHandler), 
            version: "0.0.1".to_string() 
        }, 
        conditioner: None };


    let config = ServerConfig {
        // part of the config needs to be shared between the client and server
        shared: shared_config(),
        // we can specify multiple net configs here, and the server will listen on all of them
        // at the same time. Here we will only use one
        net: vec![steam_config, net_config],
        replication: ReplicationConfig {
            // we will send updates to the clients every 100ms
            ..default()
        },
        ..default()
    };
    ServerPlugins::new(config)
}

impl Plugin for ExampleServerPlugin {
    fn build(&self, app: &mut App) {
        // add lightyear plugins
        app.add_plugins(build_server_plugin(self.steam_client.clone()));

        app.add_systems(
            PreUpdate,
            // this system will replicate the inputs of a client to other clients
            // so that a client can predict other clients
            (
                replicate_inputs
                    .after(InputSystemSet::ReceiveInputs)
                    .run_if(
                        in_state(MultiplayerState::Server)
                            .or(in_state(MultiplayerState::HostServer)),
                    ),
                // replicate_inputs_host_server.after(InputSystemSet::ReceiveInputs).run_if(in_state(MultiplayerState::HostServer)),
            ),
        );

        app.add_systems(
            FixedPreUpdate,
            replicate_inputs_host_server.run_if(in_state(MultiplayerState::HostServer)),
        );

        // the physics/FixedUpdates systems that consume inputs should be run in this set
        app.add_systems(
            FixedUpdate,
            (
                player_movement.run_if(
                    in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer)),
                ),
               
            )
                .chain(),
        );

        // Re-adding Replicate components to client-replicated entities must be done in this set for proper handling.
        app.add_systems(
            Update,
            (handle_connections,update_player_metrics).run_if(
                in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer)),
            ),
        );

    }
}

pub fn setup_server(
    mut commands: Commands,
    mut server_config: ResMut<ServerConfig>,
) {
    let port = "5000".parse::<u16>().unwrap();

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);

    // The IoConfig will specify the transport to use.
    let io = IoConfig {
        // the address specified here is the server_address, because we open a UDP socket on the server
        transport: ServerTransport::UdpSocket(server_addr),
        // conditioner: Some(LinkConditionerConfig { incoming_latency: Duration::from_millis(200), incoming_jitter:  Duration::from_millis(20), incoming_loss: 0.05 }),
        ..default()
    };
    // The NetConfig specifies how we establish a connection with the server.
    // We can use either Steam (in which case we will use steam sockets and there is no need to specify
    // our own io) or Netcode (in which case we need to specify our own io).
    let net_config = NetConfig::Netcode {
        io,
        config: NetcodeConfig::default(),
    };

    server_config.net.remove(1);
    server_config.net.push(net_config);


    // Start the server
    commands.start_server();
}



/// By default, all connection requests are accepted by the server.
#[derive(Debug, Clone)]
pub struct GnomellaConnectionRequestHandler;

impl ConnectionRequestHandler for GnomellaConnectionRequestHandler {
    fn handle_request(&self, client_id: ClientId) -> Option<DeniedReason> {
        None
    }
}

/// Since Player is replicated, this allows the clients to display remote players' latency stats.
fn update_player_metrics(
    connection_manager: Res<ConnectionManager>,
    mut q: Query<(Entity, &mut PlayerNetworkInfo)>,
) {
    for (_e, mut player) in q.iter_mut() {
        if let Ok(connection) = connection_manager.connection(player.client_id) {
            player.rtt = connection.rtt();
            player.jitter = connection.jitter();
        }
    }
}
/// Read inputs and move players
///
/// If we didn't receive the input for a given player, we do nothing (which is the default behaviour from lightyear),
/// which means that we will be using the last known input for that player
/// (i.e. we consider that the player kept pressing the same keys).
/// see: https://github.com/cBournhonesque/lightyear/issues/492
pub(crate) fn player_movement(
    mut q: Query<
        (
            &ActionState<PlayerActions>,
            ApplyInputsQuery,
        ),
        With<PlayerNetworkInfo>,
    >,
    tick_manager: Res<TickManager>,
) {
    let _tick = tick_manager.tick();
    for (action_state, mut aiq) in q.iter_mut() {

        shared_movement_behaviour(aiq.velocity, action_state);
    }
}

pub(crate) fn replicate_inputs(
    mut receive_inputs: ResMut<Events<ServerReceiveMessage<InputMessage<PlayerActions>>>>,
    mut send_inputs: EventWriter<ServerSendMessage<InputMessage<PlayerActions>>>,
) {
    // rebroadcast the input to other clients
    // we are calling drain() here so make sure that this system runs after the `ReceiveInputs` set,
    // so that the server had the time to process the inputs

    let messages = receive_inputs.drain().map(|ev| {
        ServerSendMessage::new_with_target::<InputChannel>(
            ev.message,
            NetworkTarget::AllExceptSingle(ev.from),
        )
    });
    send_inputs.send_batch(messages);
}

#[derive(Default)]
pub struct NumTicks {
    last_end_tick: Tick,
}

pub(crate) fn replicate_inputs_host_server(
    mut q_local_inputs: Query<
        (
            Entity,
            &ActionState<PlayerActions>,
            &mut InputBuffer<PlayerActions>,
        ),
        With<Controlled>,
    >,
    mut send_inputs: EventWriter<ServerSendMessage<InputMessage<PlayerActions>>>,
    tick_manager: Res<TickManager>,
    mut local: Local<NumTicks>,
) {
    let num_ticks = tick_manager.tick() - local.last_end_tick;

    let mut messages_vec = Vec::new();

    if num_ticks > 0 {
        for (entity, action, mut input_buff) in q_local_inputs.iter_mut() {
            input_buff.set(tick_manager.tick(), action);
            let target = InputTarget::Entity(entity);
            let mut input_message = InputMessage::<PlayerActions>::new(tick_manager.tick());
            input_message.add_inputs(num_ticks as u16, target, &input_buff);
            messages_vec.push(input_message);
        }

        let send = messages_vec.iter().map(|ev| {
            ServerSendMessage::new_with_target::<InputChannel>(ev.clone(), NetworkTarget::All)
        });

        // rebroadcast the input to other clients
        // we are calling drain() here so make sure that this system runs after the `ReceiveInputs` set,
        // so that the server had the time to process the inputs
        send_inputs.send_batch(send);
    }

    local.last_end_tick = tick_manager.tick();
}





/// Whenever a new client connects, spawn their spaceship
pub(crate) fn handle_connections(
    mut connections: EventReader<ConnectEvent>,
    mut commands: Commands,
    all_players: Query<Entity, With<PlayerNetworkInfo>>,
    server_setup_info: Res<crate::ClientConfigInfo>,
) {
    // track the number of connected players in order to pick colors and starting positions
    let mut player_n = all_players.iter().count();
    for connection in connections.read() {
        let client_id = connection.client_id;
        info!("New connected client, client_id: {client_id:?}. Spawning player entity..");
        // replicate newly connected clients to all players
        let replicate = Replicate {
            sync: SyncTarget {
                prediction: NetworkTarget::All,
                ..default()
            },
            controlled_by: ControlledBy {
                target: NetworkTarget::Single(client_id),
                ..default()
            },
            // make sure that all entities that are predicted are part of the same replication group
            group: REPLICATION_GROUP,
            ..default()
        };
        // pick color and x,y pos for player

      
        // } else {
        //     mask_layer = CollisionLayers::new([GameLayer::DamageTakerTeam2], [ GameLayer::DamageDealerTeam1]);
        //     team = 2;
        // }

        // spawn the player with ActionState - the client will add their own InputMap
        let player_ent = commands
            .spawn((
                PlayerNetworkInfo::new(
                    client_id,
                    pick_player_name(client_id.to_bits()),
                ),
                replicate,
                ActionTracker::new((
                    (20) as u16,
                    (20) as u16,
                ),
                (
                    (20) as u16,
                    (20) as u16,
                )),
                ActionState::<PlayerActions>::default(),
                Position(Vec2::new(100.0, 100.0)),
                // Transform::from_translation(Vec3::new(100.0, 100.0, 0.0)), //We need this so child colliders are initalized correctly
                // if we receive a pre-predicted entity, only send the prepredicted component back
                // to the original client
                // OverrideTargetComponent::<PrePredicted>::new(NetworkTarget::Single(client_id)),
                // not all physics components are replicated over the network, so add them on the server as well
                RigidBody::Kinematic,
                Collider::circle(8.0),
                ColliderDensity(0.2),
                LockedAxes::ROTATION_LOCKED,
                
                GameCleanUp,
            ))
            .id();

        info!("Created entity {player_ent:?} for client {client_id:?}");
        player_n += 1;
    }
}

fn pick_player_name(client_id: u64) -> String {
    let index = (client_id % NAMES.len() as u64) as usize;
    NAMES[index].to_string()
}

const NAMES: [&str; 35] = [
    "Ellen Ripley",
    "Sarah Connor",
    "Neo",
    "Trinity",
    "Morpheus",
    "John Connor",
    "T-1000",
    "Rick Deckard",
    "Princess Leia",
    "Han Solo",
    "Spock",
    "James T. Kirk",
    "Hikaru Sulu",
    "Nyota Uhura",
    "Jean-Luc Picard",
    "Data",
    "Beverly Crusher",
    "Seven of Nine",
    "Doctor Who",
    "Rose Tyler",
    "Marty McFly",
    "Doc Brown",
    "Dana Scully",
    "Fox Mulder",
    "Riddick",
    "Barbarella",
    "HAL 9000",
    "Megatron",
    "Furiosa",
    "Lois Lane",
    "Clark Kent",
    "Tony Stark",
    "Natasha Romanoff",
    "Bruce Banner",
    "Mr. T",
];
