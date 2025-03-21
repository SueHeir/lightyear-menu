//! The server side of the example.
//! It is possible (and recommended) to run the server in headless mode (without any rendering plugins).
//!
//! The server will:
//! - spawn a new player entity for each client that connects
//! - read inputs from the clients and move the player entities accordingly
//!
//! Lightyear will handle the replication of entities automatically if you add a `Replicate` component to them.
use crate::{ClientCommands, GameCleanUp, GameState, MultiplayerState, ServerCommands};
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use lightyear::connection::server::{ConnectionRequestHandler, DeniedReason};
use lightyear::prelude::client::{Confirmed, Predicted};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use lightyear::server::input::InputSystemSet;
use lightyear::transport::config::SharedIoConfig;
use lightyear::transport::LOCAL_SOCKET;

use std::f32::consts::TAU;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use super::{protocol::*, shared};
use avian2d::prelude::*;

use leafwing_input_manager::prelude::*;
use lightyear::shared::replication::components::InitialReplicated;

use super::shared::{apply_action_state_to_player_movement, shared_config, ApplyInputsQuery, CrossbeamEventApp, SERVER_ADDR};

#[derive(Resource)]
pub struct Global {
    predict_all: bool,
}



#[derive(Clone, Debug, TypePath)]
pub struct ExampleServerPlugin {
    pub(crate) predict_all: bool,

    pub steam_client: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>,
    pub option_sender: Option<Sender<Vec<u8>>>,
    pub option_reciever: Option<Receiver<Vec<u8>>>,
    pub client_recieve_commands: Option<Receiver<ClientCommands>>,
    pub server_send_commands: Sender<ServerCommands>,
}

#[derive(Resource)]
pub struct ServerCommandSender {
    pub server_commands: Sender<ServerCommands>,
}

/// Here we create the lightyear [`ServerPlugins`]
fn build_server_plugin(
    steam_client: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>,
    option_sender: Option<Sender<Vec<u8>>>,
    option_reciever: Option<Receiver<Vec<u8>>>,
) -> ServerPlugins {

    let mut net_vec = Vec::new();



    // The IoConfig will specify the transport to use.
    let io = IoConfig {
        // the address specified here is the server_address, because we open a UDP socket on the server
        transport: ServerTransport::UdpSocket(SERVER_ADDR),
        conditioner: Some(LinkConditionerConfig {
            incoming_latency: Duration::from_millis(80),
            incoming_jitter: Duration::from_millis(10),
            incoming_loss: 0.001,
        }),
        ..default()
    };
    // The NetConfig specifies how we establish a connection with the server.
    // We can use either Steam (in which case we will use steam sockets and there is no need to specify
    // our own io) or Netcode (in which case we need to specify our own io).
    let net_config = NetConfig::Netcode {
        io,
        config: NetcodeConfig::default(),
    };

    net_vec.push(net_config);

    let steam_config = NetConfig::Steam {
        steamworks_client: Some(steam_client.clone()),
        config: SteamConfig {
            app_id: 480,
            socket_config: SocketConfig::P2P { virtual_port: 5002 }, //SocketConfig::Ip { server_ip: Ipv4Addr::UNSPECIFIED, game_port: 5003, query_port: 27016 },
            max_clients: 10,
            connection_request_handler: Arc::new(GnomellaConnectionRequestHandler),
            version: "0.0.1".to_string(),
        },
        conditioner: None,
    };

    net_vec.push(steam_config);


    if let Some(sender) = option_sender {
        if let Some(receiver) = option_reciever {
            
        // The IoConfig will specify the transport to use.
           // create server app, which will be headless when we have client app in same process
           let extra_transport_configs = server::ServerTransport::Channels {
                // even if we communicate via channels, we need to provide a socket address for the client
                channels: vec![(LOCAL_SOCKET, receiver, sender)],
            };

             // The IoConfig will specify the transport to use.
            let io = IoConfig {
                // the address specified here is the server_address, because we open a UDP socket on the server
                transport: extra_transport_configs,
                conditioner: None,
                ..default()
            };

            let seperate_net = NetConfig::Netcode { config: NetcodeConfig::default(), io,};

            net_vec.push(seperate_net);


        }
    }

    let config = ServerConfig {
        // part of the config needs to be shared between the client and server
        shared: shared_config(false),
        // we can specify multiple net configs here, and the server will listen on all of them
        // at the same time. Here we will only use one
        net: net_vec,
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
        app.add_plugins(build_server_plugin(self.steam_client.clone(), self.option_sender.clone(), self.option_reciever.clone()));


        if let Some(receiver) = &self.client_recieve_commands {
            app.add_crossbeam_event(receiver.clone());
        }

        app.insert_resource(ServerCommandSender { server_commands: self.server_send_commands.clone() });

        app.add_systems(OnEnter(NetworkingState::Started), handle_server_started);

       
        

        app.add_systems(OnEnter(MultiplayerState::Server), setup_server);

        app.add_systems(OnEnter(GameState::Game), init.run_if(in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer))));

        
        // the physics/FixedUpdates systems that consume inputs should be run in this set
        app.add_systems(
            FixedUpdate,
            (player_movement, shared::shared_player_firing).chain().run_if(in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer))),
        );
        app.add_systems(
            Update,
            (
                handle_connections,
                update_player_metrics.run_if(on_timer(Duration::from_secs(1))),
            ).run_if(in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer))),
        );

        app.add_systems(FixedUpdate, handle_client_commands);
        

        app.add_systems(
            FixedUpdate,
            handle_hit_event
                .run_if(on_event::<BulletHitEvent>)
                .after(shared::process_collisions).run_if(in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer))),
        );
    }
}

pub fn setup_server(mut commands: Commands, mut server_config: ResMut<ServerConfig>) {

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

fn init(mut commands: Commands) {
    // the balls are server-authoritative
    const NUM_BALLS: usize = 6;
    for i in 0..NUM_BALLS {
        let radius = 10.0 + i as f32 * 4.0;
        let angle: f32 = i as f32 * (TAU / NUM_BALLS as f32);
        let pos = Vec2::new(125.0 * angle.cos(), 125.0 * angle.sin());
        commands.spawn(BallBundle::new(radius, pos, css::GOLD.into()));
    }
}


/// Since Player is replicated, this allows the clients to display remote players' latency stats.
fn update_player_metrics(
    connection_manager: Res<ConnectionManager>,
    mut q: Query<(Entity, &mut Player)>,
) {
    for (_e, mut player) in q.iter_mut() {
        if let Ok(connection) = connection_manager.connection(player.client_id) {
            player.rtt = connection.rtt();
            player.jitter = connection.jitter();
        }
    }
}










pub(crate) fn handle_client_commands(
    mut client_commands: EventReader<ClientCommands>,
    mut commands: Commands,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut game_state: ResMut<NextState<GameState>>,
    ) {

    for c in  client_commands.read() {
        
        match c {
            ClientCommands::StartServer => {
                info!("Server received StartServer command");
                multiplayer_state.set(MultiplayerState::Server);
                game_state.set(GameState::Game);
            },
            ClientCommands::StopServer => {
                info!("Server received StopServer command");
                commands.stop_server();
                multiplayer_state.set(MultiplayerState::None);
                game_state.set(GameState::Menu);


            },
        }
    }
}

pub(crate) fn handle_server_started(
    server_commands: Res<ServerCommandSender>,
) {
    server_commands.server_commands.send(ServerCommands::ServerStarted);

}


/// Whenever a new client connects, spawn their spaceship
pub(crate) fn handle_connections(
    mut connections: EventReader<ConnectEvent>,
    mut commands: Commands,
    all_players: Query<Entity, With<Player>>,
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

        let available_colors = [
            css::LIMEGREEN,
            css::PINK,
            css::YELLOW,
            css::AQUA,
            css::CRIMSON,
            css::GOLD,
            css::ORANGE_RED,
            css::SILVER,
            css::SALMON,
            css::YELLOW_GREEN,
            css::WHITE,
            css::RED,
        ];
        let col = available_colors[player_n % available_colors.len()];
        let angle: f32 = player_n as f32 * 5.0;
        let x = 200.0 * angle.cos();
        let y = 200.0 * angle.sin();

        // spawn the player with ActionState - the client will add their own InputMap
        let player_ent = commands
            .spawn((
                Player::new(client_id, pick_player_name(client_id.to_bits())),
                Score(0),
                Name::new("Player"),
                ActionState::<PlayerActions>::default(),
                Position(Vec2::new(x, y)),
                replicate,
                PhysicsBundle::player_ship(),
                Weapon::new((64.0 / 5.0) as u16),
                ColorComponent(col.into()),
            ))
            .id();

        // commands.entity(player_ent).with_child((
        //     Transform::from_translation(Vec3::new(0., 10., 0.)),
        //     Sensor,
        //     Collider::capsule(8.0, 20.0),
        // ));

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

/// Server will manipulate scores when a bullet collides with a player.
/// the `Score` component is a simple replication. scores fully server-authoritative.
pub(crate) fn handle_hit_event(
    connection_manager: Res<server::ConnectionManager>,
    mut events: EventReader<BulletHitEvent>,
    client_q: Query<&ControlledEntities, Without<Player>>,
    mut player_q: Query<(&Player, &mut Score)>,
) {
    let client_id_to_player_entity = |client_id: ClientId| -> Option<Entity> {
        if let Ok(e) = connection_manager.client_entity(client_id) {
            if let Ok(controlled_entities) = client_q.get(e) {
                return controlled_entities.entities().pop();
            }
        }
        None
    };

    for ev in events.read() {
        // did they hit a player?
        if let Some(victim_entity) = ev.victim_client_id.and_then(client_id_to_player_entity) {
            if let Ok((player, mut score)) = player_q.get_mut(victim_entity) {
                score.0 -= 1;
            }
            if let Some(shooter_entity) = client_id_to_player_entity(ev.bullet_owner) {
                if let Ok((player, mut score)) = player_q.get_mut(shooter_entity) {
                    score.0 += 1;
                }
            }
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
    mut q: Query<(&ActionState<PlayerActions>, ApplyInputsQuery), With<Player>>,
    tick_manager: Res<TickManager>,
) {
    let tick = tick_manager.tick();
    for (action_state, mut aiq) in q.iter_mut() {
        // if !aiq.action.get_pressed().is_empty() {
        //     info!(
        //         "🎹 {:?} {tick:?} = {:?}",
        //         aiq.player.client_id,
        //         aiq.action.get_pressed(),
        //     );
        // }
        // check for missing inputs, and set them to default? or sustain for 1 tick?
        apply_action_state_to_player_movement(action_state, 0, &mut aiq, tick);
    }
}
