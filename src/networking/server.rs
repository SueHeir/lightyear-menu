//! The server side of the example.
//! It is possible (and recommended) to run the server in headless mode (without any rendering plugins).
//!
//! The server will:
//! - spawn a new player entity for each client that connects
//! - read inputs from the clients and move the player entities accordingly
//!
//! Lightyear will handle the replication of entities automatically if you add a `Replicate` component to them.
use crate::networking::shared::shared_movement_behaviour;
use crate::{GameCleanUp, GameState, MultiplayerState};
use bevy::color::palettes::css;
use bevy::prelude::*;
use lightyear::connection::server::{ConnectionRequestHandler, DeniedReason};
use lightyear::prelude::client::{Confirmed, Predicted};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use lightyear::server::input::leafwing::InputSystemSet;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use super::protocol::*;
use avian2d::prelude::*;

use leafwing_input_manager::prelude::*;
use lightyear::shared::replication::components::InitialReplicated;

use super::shared::{
    shared_config, SERVER_ADDR,
};



#[derive(Resource)]
pub struct Global {
    predict_all: bool,
}

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

        app.insert_resource(Global {
            predict_all: self.predict_all,
        });
        // add lightyear plugins
        app.add_plugins(build_server_plugin(self.steam_client.clone()));

        // app.add_systems(OnEnter(GameState::Game), init.run_if(in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer))));

        app.add_systems(
            PreUpdate,
            // this system will replicate the inputs of a client to other clients
            // so that a client can predict other clients
            replicate_inputs.after(InputSystemSet::ReceiveInputs),
        );
        // Re-adding Replicate components to client-replicated entities must be done in this set for proper handling.
        app.add_systems(
            PreUpdate,
            replicate_players.in_set(ServerReplicationSet::ClientReplication),
        );
        // the physics/FixedUpdates systems that consume inputs should be run in this set
        app.add_systems(FixedUpdate, movement.run_if(in_state(MultiplayerState::Server).or(in_state(MultiplayerState::Server))));

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
    fn handle_request(&self, _client_id: ClientId) -> Option<DeniedReason> {
        None
    }
}

fn init(mut commands: Commands, global: Res<Global>) {
    // the ball is server-authoritative
    commands.spawn((BallBundle::new(
        Vec2::new(0.0, 0.0),
        css::AZURE.into(),
        // if true, we predict the ball on clients
        global.predict_all,
    ), GameCleanUp));
}

/// Read client inputs and move players
/// NOTE: this system can now be run in both client/server!
pub(crate) fn movement(
    tick_manager: Res<TickManager>,
    mut action_query: Query<
        (
            Entity,
            &Position,
            &mut LinearVelocity,
            &ActionState<PlayerActions>,
        ),
        // if we run in host-server mode, we don't want to apply this system to the local client's entities
        // because they are already moved by the client plugin
        (Without<Confirmed>, Without<Predicted>),
    >,
) {
    for (entity, position, velocity, action) in action_query.iter_mut() {
      
        // NOTE: be careful to directly pass Mut<PlayerPosition>
        // getting a mutable reference triggers change detection, unless you use `as_deref_mut()`
        shared_movement_behaviour(velocity, action);
        trace!(?entity, tick = ?tick_manager.tick(), ?position, actions = ?action.get_pressed(), "applying movement to player");
        
    }
}

/// When we receive the input of a client, broadcast it to other clients
/// so that they can predict this client's movements accurately
pub(crate) fn replicate_inputs(
    mut receive_inputs: ResMut<Events<ServerReceiveMessage<InputMessage<PlayerActions>>>>,
    mut send_inputs: EventWriter<ServerSendMessage<InputMessage<PlayerActions>>>,
) {
    // rebroadcast the input to other clients
    // we are calling drain() here so make sure that this system runs after the `ReceiveInputs` set,
    // so that the server had the time to process the inputs
    send_inputs.send_batch(receive_inputs.drain().map(|ev| {
        ServerSendMessage::new_with_target::<InputChannel>(
            ev.message,
            NetworkTarget::AllExceptSingle(ev.from),
        )
    }));
}

// Replicate the pre-predicted entities back to the client
// We have to use `InitialReplicated` instead of `Replicated`, because
// the server has already assumed authority over the entity so the `Replicated` component
// has been removed
pub(crate) fn replicate_players(
    global: Res<Global>,
    mut commands: Commands,
    query: Query<(Entity, &InitialReplicated), (Added<InitialReplicated>, With<PlayerId>)>,
) {
    for (entity, replicated) in query.iter() {
        let client_id = replicated.client_id();
        info!(
            "Received player spawn event from client {client_id:?}. Replicating back to all clients"
        );

        // for all player entities we have received, add a Replicate component so that we can start replicating it
        // to other clients
        if let Some(mut e) = commands.get_entity(entity) {
            // we want to replicate back to the original client, since they are using a pre-predicted entity
            let mut sync_target = SyncTarget::default();

            if global.predict_all {
                sync_target.prediction = NetworkTarget::All;
            } else {
                // we want the other clients to apply interpolation for the player
                sync_target.interpolation = NetworkTarget::AllExceptSingle(client_id);
            }
            let replicate = Replicate {
                sync: sync_target,
                controlled_by: ControlledBy {
                    target: NetworkTarget::Single(client_id),
                    ..default()
                },
                // make sure that all entities that are predicted are part of the same replication group
                group: REPLICATION_GROUP,
                ..default()
            };
            e.insert((
                replicate,
                GameCleanUp,
                // if we receive a pre-predicted entity, only send the prepredicted component back
                // to the original client
                OverrideTargetComponent::<PrePredicted>::new(NetworkTarget::Single(client_id)),
                // not all physics components are replicated over the network, so add them on the server as well
                PhysicsBundle::player(),
            ));
        }
    }
}
