//! The server side of the example.
//! It is possible (and recommended) to run the server in headless mode (without any rendering plugins).
//!
//! The server will:
//! - spawn a new player entity for each client that connects
//! - read inputs from the clients and move the player entities accordingly
//!
//! Lightyear will handle the replication of entities automatically if you add a `Replicate` component to them.
use std::sync::Arc;
use std::time::Duration;

use crate::networking::protocol::BallMarker;
use crate::networking::protocol::BulletHitEvent;
use crate::networking::protocol::ColorComponent;
use crate::networking::protocol::PhysicsBundle;
use crate::networking::protocol::Player;
use crate::networking::protocol::PlayerActions;
use crate::networking::protocol::Score;
use crate::networking::protocol::Weapon;
use crate::networking::shared;
use crate::networking::shared::*;
use crate::ClientCommands;
use crate::GameState;
use crate::MultiplayerState;
use crate::ServerCommands;
use avian2d::prelude::Position;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;
use crossbeam_channel::Receiver;
use crossbeam_channel::Sender;
use leafwing_input_manager::prelude::ActionState;
use lightyear::connection::client::PeerMetadata;
use lightyear::crossbeam::CrossbeamIo;
use lightyear::link::Unlink;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use parking_lot::Mutex;
use std::f32::consts::TAU;
use steamworks::LobbyId;

#[derive(Resource)]
pub struct ServerCommandSender {
    pub server_commands: Sender<ServerCommands>,
}

#[derive(Resource)]
pub struct ServerStartupResources {
    pub just_server: bool,
    pub server_crossbeam: Option<CrossbeamIo>,
    pub steam_lobby_id:
        Option<Arc<parking_lot::lock_api::Mutex<parking_lot::RawMutex, Option<LobbyId>>>>,
}

#[derive(Resource)]
pub struct SteamSingleClient {
    pub steam: Arc<Mutex<lightyear::prelude::steamworks::SingleClient>>,
}

pub struct ExampleServerPlugin {
    pub just_server: bool,
    pub server_crossbeam: Option<CrossbeamIo>,
    pub client_recieve_commands: Option<Receiver<ClientCommands>>,
    pub server_send_commands: Option<Sender<ServerCommands>>,
    pub steam: Option<lightyear::prelude::steamworks::Client>,
    pub wrapped_single_client: Option<Arc<Mutex<lightyear::prelude::steamworks::SingleClient>>>,
}

#[derive(Resource)]
pub struct Global {
    predict_all: bool,
}

impl Plugin for ExampleServerPlugin {
    fn build(&self, app: &mut App) {
        // Create the server immediately
        let server_entity = app
            .world_mut()
            .spawn((
                NetcodeServer::new(NetcodeConfig::default()),
                LocalAddr(SERVER_ADDR),
                ServerUdpIo::default(),
            ))
            .id();

        app.insert_resource(ServerStartupResources {
            server_crossbeam: self.server_crossbeam.clone(),
            steam_lobby_id: None,
            just_server: self.just_server,
        });

        if self.steam.is_some() && self.wrapped_single_client.is_some() {
            info!("Setting up Steamworks for server connection");

            let steam = self.steam.clone().unwrap();
            let wrapped_single_client = self.wrapped_single_client.clone().unwrap();

            app.insert_resource(SteamworksClient(steam.clone()));

            let resource = SteamSingleClient {
                steam: wrapped_single_client.clone(),
            };
            app.insert_resource(resource);
            app.add_systems(
                PreUpdate,
                steam_callbacks.run_if(in_state(MultiplayerState::Server)),
            );

            // If the server is using Steamworks, we need to add the SteamServerIo component
            app.world_mut()
                .entity_mut(server_entity)
                .insert(SteamServerIo {
                    target: ListenTarget::Peer { virtual_port: 4001 },
                    config: SessionConfig::default(),
                });
        }

        if self.client_recieve_commands.is_some() {
            app.add_crossbeam_event(self.client_recieve_commands.clone().unwrap().clone());
            app.add_observer(handle_server_started);
        }
        if self.server_send_commands.is_some() {
            app.insert_resource(ServerCommandSender {
                server_commands: self.server_send_commands.clone().unwrap().clone(),
            });
            app.add_systems(FixedUpdate, handle_client_commands);
        }

        // app.add_systems(OnEnter(GameState::Game), init.run_if(in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer))));
        app.add_systems(OnEnter(MultiplayerState::Server), start_server);

        app.insert_resource(Global { predict_all: true });
        app.add_systems(OnEnter(MultiplayerState::Server), init);
        // the physics/FixedUpdates systems that consume inputs should be run in this set
        app.add_systems(
            FixedUpdate,
            (player_movement, shared::shared_player_firing).chain(),
        );
        app.add_observer(handle_new_client);
        app.add_observer(handle_connections);
        app.add_systems(
            Update,
            (update_player_metrics.run_if(on_timer(Duration::from_secs(1))),),
        );

        app.add_systems(
            FixedUpdate,
            handle_hit_event
                .run_if(on_event::<BulletHitEvent>)
                .after(shared::process_collisions),
        );

        app.add_systems(Update, talk_to_me);
    }
}

fn talk_to_me(server_q: Query<(Entity, Has<RemoteId>, Has<ReplicationSender>), With<Link>>) {

    // for (e, remote, replication) in server_q {
    //     info!(" remote:{} rep:{}", remote, replication);
    // }
}

fn steam_callbacks(steam: ResMut<SteamSingleClient>, server_q: Query<Entity, With<Started>>) {
    if server_q.is_empty() {
        // If the server is not started, we don't need to run the callbacks
        return;
    }
    // This system is responsible for running the Steamworks callbacks
    // It should be run every frame to ensure that the Steamworks API works correctly
    steam.steam.lock().run_callbacks();
}

pub fn start_server(
    mut commands: Commands,
    server_q: Query<Entity, With<Server>>,
    mut server_startup: ResMut<ServerStartupResources>,
    steam_works: Option<Res<SteamworksClient>>,
) {
    if let Some(server) = server_q.iter().next() {
        commands.trigger_targets(Start, server);

        if !server_startup.just_server {
            if let Some(server_crossbeam) = &server_startup.server_crossbeam {
                // You need to provide a valid client_id here. For demonstration, we'll use 12345.
                info!("Add a Linked connection for host client to server");

                let mut entity = commands.spawn(LinkOf { server: server });
                entity.insert(PingManager::new(PingConfig {
                    ping_interval: Duration::default(),
                }));
                entity.insert(Link::new(None));
                entity.insert(Linked);
                entity.insert(server_crossbeam.clone());
                entity.insert(RemoteId(PeerId::Netcode(1)));
            }
        }

        if let Some(steam_work) = steam_works {
            let shared_data: Arc<
                parking_lot::lock_api::Mutex<parking_lot::RawMutex, Option<LobbyId>>,
            > = Arc::new(Mutex::new(None));
            let cloned_data = shared_data.clone();
            steam_work.matchmaking().create_lobby(
                steamworks::LobbyType::FriendsOnly,
                10,
                move |result: Result<LobbyId, steamworks::SteamError>| {
                    match result {
                        Ok(lobby_id) => {
                            shared_data.clone().lock().replace(lobby_id);
                            println!("{:?}", lobby_id);
                            // Do something with the LobbyId, like joining it, setting metadata, etc.
                        }
                        Err(e) => {
                            eprintln!("Error creating lobby: {:?}", e);
                        }
                    }
                },
            );

            server_startup.steam_lobby_id = Some(cloned_data);
            info_once!("{:?}", server_startup.steam_lobby_id);
        }

        info!("Server Started");
    } else {
        error!("No server entity found to set up");
        return;
    }
    // Start the server
    // commands.start_server();
}

pub(crate) fn handle_server_started(
    _trigger: Trigger<OnAdd, Started>,
    server_commands: Res<ServerCommandSender>,
) {
    let _ = server_commands
        .server_commands
        .send(ServerCommands::ServerStarted);
}

pub(crate) fn handle_client_commands(
    mut client_commands: EventReader<ClientCommands>,
    mut commands: Commands,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut game_state: ResMut<NextState<GameState>>,
    mut server_q: Query<Entity, With<Server>>,
    mut server_startup: ResMut<ServerStartupResources>,
    steam_works: Option<Res<SteamworksClient>>,
) {
    for c in client_commands.read() {
        match c {
            ClientCommands::StartServer => {
                info!("Server received StartServer command");
                multiplayer_state.set(MultiplayerState::Server);
                game_state.set(GameState::Game);
            }
            ClientCommands::StopServer => {
                info!("Server received StopServer command");
                if let Some(server) = server_q.iter().next() {
                    if let Some(ref steam_work) = steam_works {
                        if let Some(lobby_arc) = server_startup.steam_lobby_id.clone() {
                            if let Some(lobby_id) = *lobby_arc.lock() {
                                steam_work.matchmaking().leave_lobby(lobby_id);
                            }
                        }
                        server_startup.steam_lobby_id = None;
                    }

                    commands.trigger_targets(
                        Unlink {
                            reason: "Stopping Server".to_string(),
                        },
                        server,
                    );
                    commands.trigger_targets(Stop, server);

                    info!("Server Stopped");
                }
                multiplayer_state.set(MultiplayerState::None);
                game_state.set(GameState::Menu);
            }
        }
    }
}

/// Since Player is replicated, this allows the clients to display remote players' latency stats.
fn update_player_metrics(
    links: Query<&Link, With<LinkOf>>,
    mut q: Query<(&mut Player, &ControlledBy)>,
) {
    for (mut player, controlled) in q.iter_mut() {
        if let Ok(link) = links.get(controlled.owner) {
            player.rtt = link.stats.rtt;
            player.jitter = link.stats.jitter;
        }
    }
}

fn init(mut commands: Commands) {
    // the balls are server-authoritative
    const NUM_BALLS: usize = 6;
    for i in 0..NUM_BALLS {
        let radius = 10.0 + i as f32 * 4.0;
        let angle: f32 = i as f32 * (TAU / NUM_BALLS as f32);
        let pos = Vec2::new(125.0 * angle.cos(), 125.0 * angle.sin());
        let ball = BallMarker::new(radius);
        commands.spawn((
            Position(pos),
            ColorComponent(css::GOLD.into()),
            ball.physics_bundle(),
            ball,
            Name::new("Ball"),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ));
    }
}

/// Add the ReplicationSender component to new clients
pub(crate) fn handle_new_client(trigger: Trigger<OnAdd, ClientOf>, mut commands: Commands) {
    info!(
        "remote id added, adding replication sender {}",
        trigger.target()
    );
    commands
        .entity(trigger.target())
        .insert(ReplicationSender::new(
            SERVER_REPLICATION_INTERVAL,
            SendUpdatesMode::SinceLastAck,
            false,
        ));
}

/// Whenever a new client connects, spawn their spaceship
pub(crate) fn handle_connections(
    trigger: Trigger<OnAdd, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
    all_players: Query<Entity, With<Player>>,
) {
    // track the number of connected players in order to pick colors and starting positions
    let player_n = all_players.iter().count();
    if let Ok(remote_id) = query.get(trigger.target()) {
        let client_id = remote_id.0;
        info!("New connected client, client_id: {client_id:?}. Spawning player entity..");
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
                Replicate::to_clients(NetworkTarget::All),
                PredictionTarget::to_clients(NetworkTarget::All),
                ControlledBy {
                    owner: trigger.target(),
                    lifetime: Default::default(),
                },
                // prevent rendering children to be replicated
                DisableReplicateHierarchy,
                PhysicsBundle::player_ship(),
                Weapon::new((FIXED_TIMESTEP_HZ / 5.0) as u16),
                ColorComponent(col.into()),
            ))
            .id();
        info!("Created entity {player_ent:?} for client {client_id:?}");
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
/// the `Score` component is a simple replication. Score is fully server-authoritative.
pub(crate) fn handle_hit_event(
    peer_metadata: Res<PeerMetadata>,
    mut events: EventReader<BulletHitEvent>,
    mut player_q: Query<(&Player, &mut Score)>,
) {
    let client_id_to_player_entity =
        |client_id: PeerId| -> Option<Entity> { peer_metadata.mapping.get(&client_id).copied() };

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
    timeline: Single<&LocalTimeline, With<Server>>,
) {
    let tick = timeline.tick();
    for (action_state, mut aiq) in q.iter_mut() {
        if !action_state.get_pressed().is_empty() {
            trace!(
                "🎹 {:?} {tick:?} = {:?}",
                aiq.player.client_id,
                action_state.get_pressed(),
            );
        }
        apply_action_state_to_player_movement(action_state, &mut aiq, tick);
    }
}
