//! The client plugin.
use crate::networking::protocol::{BallMarker, BulletHitEvent, BulletMarker, PhysicsBundle, Player, PlayerActions};
use crate::networking::server::SteamSingleClient;
use crate::networking::shared::*;
use crate::{ClientCommands, ClientConfigInfo, GameState, MultiplayerState, ServerCommands};
use avian2d::prelude::Collider;
use bevy::prelude::*;
use leafwing_input_manager::prelude::{ActionState, InputMap};
use lightyear::crossbeam::CrossbeamIo;
use parking_lot::Mutex;
use steamworks::{Callback, GameLobbyJoinRequested, LobbyId};
use core::net::Ipv4Addr;
use core::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use lightyear::netcode::Key;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use lightyear::prelude::PeerId::Steam;
// use bevy::ecs::schedule::common_conditions::any_of;
// use bevy::ecs::schedule::common_conditions::in_states;



#[derive(Resource)]
pub struct ClientStartupResources {
    pub client_crossbeam: Option<CrossbeamIo>,
    pub client_sender_commands: Option<crossbeam_channel::Sender<ClientCommands>>,
    pub steam_accept_join_game_request: Option<Arc<parking_lot::lock_api::Mutex<parking_lot::RawMutex, Option<SteamId>>>>,
    
}
pub struct ExampleClientPlugin {
    pub client_crossbeam: Option<CrossbeamIo>,
    pub client_sender_commands: Option<crossbeam_channel::Sender<ClientCommands>>,
    pub server_receive_commands: Option<crossbeam_channel::Receiver<ServerCommands>>,
    pub steam: Option<lightyear::prelude::steamworks::Client>,
    pub wrapped_single_client: Option<Arc<Mutex<lightyear::prelude::steamworks::SingleClient>>>,
    
}

const CLIENT_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4000);

impl Plugin for ExampleClientPlugin {
    fn build(&self, app: &mut App) {
        // add our client-specific logic. Here we will just connect to the server
        
        app.insert_resource(ClientStartupResources {
            client_crossbeam: self.client_crossbeam.clone(),
            client_sender_commands: self.client_sender_commands.clone(),
            steam_accept_join_game_request: None,
        });

        app.add_systems(Startup, temp_client);
        app.add_systems(OnEnter(GameState::Menu), setup_steam_callbacks);
        app.add_crossbeam_event(self.server_receive_commands.clone().unwrap());



        

        if self.steam.is_some() && self.wrapped_single_client.is_some() {

            info!("Using Steamworks for server connection");

            let steam = self.steam.clone().unwrap();
            let wrapped_single_client = self.wrapped_single_client.clone().unwrap();
            

            app.insert_resource(SteamworksClient(steam.clone()));


            let resource = SteamSingleClient {
                steam: wrapped_single_client.clone(),
            };
            app.insert_resource(resource);
            app.add_systems(
                PreUpdate,
                steam_callbacks);

        }
        
        

        app.add_systems(OnEnter(MultiplayerState::ClientSpawnServer), client_start_server);
        app.add_systems(FixedUpdate, handle_server_commands);
        app.add_systems(OnEnter(MultiplayerState::Client), client_connect);
        app.add_systems(
            FixedUpdate,
            clean_up_game_on_client_disconnect.run_if(
                    in_state(MultiplayerState::Client),
                
            ),
        );
        app.add_systems(Update, esc_to_disconnect.run_if(
            in_state(MultiplayerState::Client),
        ));
       
        
        app.add_systems(
            PreUpdate,
            client_stop_server
        );




        app.add_systems(FixedUpdate, (player_movement, shared_player_firing).chain());
        app.add_observer(add_ball_physics);
        app.add_observer(add_bullet_physics);
        app.add_observer(handle_new_player);

        app.add_systems(
            FixedUpdate,
            handle_hit_event
                .run_if(on_event::<BulletHitEvent>)
                .after(process_collisions),
        );

    }
}

fn steam_callbacks(
    steam: ResMut<SteamSingleClient>,
    client_config: Res<ClientConfigInfo>, 
) {
    // This system is responsible for running the Steamworks callbacks
    // It should be run every frame to ensure that the Steamworks API works correctly
    // if client_config.seperate_mode {
    //     // If we are in seperate mode, we don't need to run the callbacks
    //     return;
    // }

    steam.steam.lock().run_callbacks();
}

pub fn esc_to_disconnect(
    keys: Res<ButtonInput<KeyCode>>,
    multiplayer_state: Res<State<MultiplayerState>>,
    mut client_startup: ResMut<ClientStartupResources>,
    mut game_state: ResMut<NextState<GameState>>,
    client_q: Query<Entity, With<Client>>,
    client_config: Res<ClientConfigInfo>, 
    mut commands: Commands,
) {
    if let Ok(client) = client_q.single_inner() {
        if keys.just_pressed(KeyCode::Escape) {
            if MultiplayerState::Client == *multiplayer_state.get() {
                commands.trigger_targets(Disconnect, client);
            }
        }
    }
}
fn temp_client(mut commands: Commands) {

    let client = commands.spawn( (
            Name::new("Client"), 
            Client::default(),
            ReplicationReceiver::default(),
            PredictionManager::default(),
            InterpolationManager::default(),
    )).id();
}

fn setup_steam_callbacks(mut commands: Commands, mut client_startup: ResMut<ClientStartupResources>,  steam_works: Option<Res<SteamworksClient>>) -> Result {

   

    if let Some(steam_work) = steam_works {


        let shared_data: Arc<parking_lot::lock_api::Mutex<parking_lot::RawMutex, Option<SteamId>>> = Arc::new(Mutex::new(None));
        let cloned_data = shared_data.clone();


        let _lobby_join_callback = steam_work.register_callback(
           move |p: GameLobbyJoinRequested| { // The closure takes a GameLobbyJoinRequested struct as an argument
                shared_data.lock().replace(p.friend_steam_id);
        });


        client_startup.steam_accept_join_game_request = Some(cloned_data);
    }




    Ok(())
}



fn client_start_server(mut client_startup: ResMut<ClientStartupResources>) {

    // We need to send a command to the server to start the server
    if let Some(sender) = &client_startup.client_sender_commands {
        let _result = sender.send(ClientCommands::StartServer);
    } else {
        error!("client_sender_commands is None, cannot send StartServer command");
    }

}


fn client_stop_server(client_config: Res<ClientConfigInfo>, mut client_startup: ResMut<ClientStartupResources>,  client_q: Query<(Entity, &Client), Added<Disconnected>>,) {
    if !client_config.seperate_mode {
        // If we are in seperate mode, we don't need to stop the server
        return;
    }

    if let Some(client) = client_q.single_inner().ok() {
        info!("Client disconnected, cleaning up game state");
         if let Some(sender) = &client_startup.client_sender_commands {
            // let _result = sender.send(ClientCommands::StopServer);
        } else {
            error!("client_sender_commands is None, cannot send StartServer command");
        }
    } 
    // We need to send a command to the server to start the server
   

}

fn handle_server_commands(
    mut client_commands: EventReader<ServerCommands>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    ) {

    for c in  client_commands.read() {
        
        match c {
            ServerCommands::ServerStarted => {
                info!("client knows server is started!");
                multiplayer_state.set(MultiplayerState::Client);
            },
        }
    }
}


/// Trigger Client to connect to the server
fn client_connect(
    mut commands: Commands, 
    client_q: Query<Entity, With<Client>>,
    client_config: Res<ClientConfigInfo>, 
    mut client_startup: ResMut<ClientStartupResources>,
    steam_works: Option<Res<SteamworksClient>>) -> Result {
    
    // let client = client_q.single_inner().ok().unwrap();

    // commands.entity(client).try_remove::<CrossbeamIo>()
    //     .try_remove::<SteamClientIo>()
    //     .try_remove::<UdpIo>()
    //     .try_remove::<NetcodeClient>()
    //     .try_remove::<Linked>()
    //     .try_remove::<Link>()
    //     .try_remove::<PingManager>();

    for e in client_q.iter() {
        commands.entity(e).try_despawn();
    }

    let client = commands.spawn( (
            Name::new("Client"), 
            Client::default(),
            ReplicationReceiver::default(),
            PredictionManager::default(),
            InterpolationManager::default(),
    )).id();

    if client_config.seperate_mode {

        let auth = Authentication::Manual {
            server_addr: SERVER_ADDR,
            client_id: 1,
            private_key: Key::default(),
            protocol_id: 0,
        };
       

        commands.entity(client).insert((
           PingManager::new(PingConfig {
                ping_interval: Duration::default(),
            }),
            NetcodeClient::new(auth, NetcodeConfig::default())?,
            client_startup.client_crossbeam.clone().unwrap(), 
            LocalAddr(CLIENT_ADDR),
            PeerAddr(SERVER_ADDR),
            Link::new(None), // This is the link to the server, which will be established when the client connects
        ));

        commands.trigger_targets(Connect, client);

        info!("Using CrossbeamIo for client connection");
        return Ok(());
    }

    if client_config.steam_connect_to.is_some() {
        // Connect to the server using Steamworks
        // let steam_client = commands
        //     .get_resource::<Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>>()
        //     .unwrap();
        // let steam_client = steam_client.read();
        // let _ = steam_client.connect_to(client_config.steam_connect_to.unwrap());
        let auth = Authentication::Manual {
            server_addr: SERVER_ADDR,
            client_id: rand::random::<u64>(),
            private_key: Key::default(),
            protocol_id: 0,
        };

        commands.entity(client).insert((
            NetcodeClient::new(auth, NetcodeConfig::default())?,
            SteamClientIo { target: ConnectTarget::Peer { steam_id: client_config.steam_connect_to.unwrap().0, virtual_port: 4001 }, config: SessionConfig::default() },
            RemoteId(Steam(client_config.steam_connect_to.unwrap().0.raw())),
            Link::new(None), // This is the link to the server, which will be established when the client connects
        ));

        // if let Some(steam_work) = steam_works {
        //     steam_work.matchmaking().join_lobby(client_config.steam_connect_to.unwrap().1, 
        //     |result: Result<LobbyId, ()>| {
        //             match result {
        //                 Ok(lobby_id) => {
        //                     println!("{:?}", lobby_id);
        //                     // Do something with the LobbyId, like joining it, setting metadata, etc.
        //                 }
        //                 Err(e) => {
        //                     eprintln!("Error joining lobby: {:?}", e);
        //                 }
        //             }
        //         },);
        // }



        commands.trigger_targets(Connect, client);
        info!("Using Steam for client connection");

        return Ok(());
    } 


    let auth = Authentication::Manual {
        server_addr: SERVER_ADDR,
        client_id: rand::random::<u64>(),
        private_key: Key::default(),
        protocol_id: 0,
    };

    // Connect to the server using standard udp
    commands.entity(client).insert((
        Link::new(None),
        UdpIo::default(), 
        NetcodeClient::new(auth, NetcodeConfig::default())?,
        LocalAddr(CLIENT_ADDR),
        PeerAddr(SERVER_ADDR),
    ));

    commands.trigger_targets(Connect, client);

    info!("Using Udp for client connection");
    Ok(())
}


pub fn clean_up_game_on_client_disconnect(
    client_q: Query<Entity, With<Disconnected>>,
    client_startup: Res<ClientStartupResources>,
    mut game_state: ResMut<NextState<GameState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
) {
    if let Some(_client) = client_q.single_inner().ok() {
        // info!("Client disconnected, cleaning up game state");
        game_state.set(GameState::Menu);
        multiplayer_state.set(MultiplayerState::None);
        // // Despawn the client entity
        // commands.despawn(client);
        if let Some(sender) = &client_startup.client_sender_commands {
            let _result = sender.send(ClientCommands::StopServer);
        } else {
            error!("client_sender_commands is None, cannot send StopServer command");
        }
    } 
}





/// When the ball gets replicated from the server, add all the components
/// that we need that are not replicated.
/// (for example physical properties that are constant, so they don't need to be networked)
///
/// We only add the physical properties on the ball that is displayed on screen (i.e the Predicted ball)
/// We want the ball to be rigid so that when players collide with it, they bounce off.
fn add_ball_physics(
    trigger: Trigger<OnAdd, BallMarker>,
    ball_query: Query<&BallMarker, With<Predicted>>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    if let Ok(ball) = ball_query.get(entity) {
        info!("Adding physics to a replicated ball {entity:?}");
        commands.entity(entity).insert(ball.physics_bundle());
    }
}

/// Simliar blueprint scenario as balls, except sometimes clients prespawn bullets ahead of server
/// replication, which means they will already have the physics components.
/// So, we filter the query using `Without<Collider>`.
fn add_bullet_physics(
    trigger: Trigger<OnAdd, BulletMarker>,
    mut commands: Commands,
    bullet_query: Query<(), (With<Predicted>, Without<Collider>)>,
) {
    let entity = trigger.target();
    if let Ok(()) = bullet_query.get(entity) {
        info!("Adding physics to a replicated bullet: {entity:?}");
        commands.entity(entity).insert(PhysicsBundle::bullet());
    }
}

/// Decorate newly connecting players with physics components
/// ..and if it's our own player, set up input stuff
fn handle_new_player(
    trigger: Trigger<OnAdd, (Player, Predicted)>,
    mut commands: Commands,
    player_query: Query<(&Player, Has<Controlled>), With<Predicted>>,
) {
    let entity = trigger.target();
    if let Ok((player, is_controlled)) = player_query.get(entity) {
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
        }
        commands.entity(entity).insert(PhysicsBundle::player_ship());
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
    mut q: Query<(&ActionState<PlayerActions>, ApplyInputsQuery), (With<Player>, With<Predicted>)>,
    timeline: Single<&LocalTimeline, With<PredictionManager>>,
) {
    // get the tick, even if during rollback
    let tick = timeline.tick();

    for (action_state, mut aiq) in q.iter_mut() {
        if !action_state.get_pressed().is_empty() {
            trace!(
                "🎹 {:?} {tick:?} = {:?}",
                aiq.player.client_id,
                action_state.get_pressed(),
            );
        }
        // if we haven't received any input for some tick, lightyear will predict that the player is still pressing the same keys.
        // (it does that by not modifying the ActionState, so it will still have the last pressed keys)
        apply_action_state_to_player_movement(action_state, &mut aiq, tick);
    }
}
