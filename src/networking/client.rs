//! The client plugin.
use crate::networking::server::SteamSingleClient;
use crate::networking::shared::*;
use crate::{ClientCommands, ClientConfigInfo, GameState, MultiplayerState, ServerCommands};
use bevy::prelude::*;
use lightyear::crossbeam::CrossbeamIo;
use parking_lot::Mutex;
use core::net::Ipv4Addr;
use core::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use lightyear::netcode::Key;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
// use bevy::ecs::schedule::common_conditions::any_of;
// use bevy::ecs::schedule::common_conditions::in_states;



#[derive(Resource)]
pub struct ClientStartupResources {
    pub client_crossbeam: Option<CrossbeamIo>,
    pub client_sender_commands: Option<crossbeam_channel::Sender<ClientCommands>>,
    
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
        });

        app.add_systems(Startup, spawn_client);
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


/// Spawn a client that connects to the server
fn spawn_client(mut commands: Commands, mut client_startup: ResMut<ClientStartupResources>) -> Result {
    let auth = Authentication::Manual {
        server_addr: SERVER_ADDR,
        client_id: 1,
        private_key: Key::default(),
        protocol_id: 0,
    };
    commands
        .spawn((
            Name::new("Client"),
            Client::default(),
            

            //Example CrossbeamIo Client
            // Client::default(),
            // // Send pings every frame, so that the Acks are sent every frame
            // PingManager::new(PingConfig {
            //     ping_interval: Duration::default(),
            // }),
            // ReplicationSender::default(),
            // ReplicationReceiver::default(),
            // NetcodeClient::new(auth, NetcodeConfig::default()).unwrap(),
            // crossbeam_client,
            // TestHelper::default(),
            // PredictionManager::default(),
        ));
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
    mut client_startup: ResMut<ClientStartupResources>) -> Result {
    
    let client = client_q.single_inner().ok().unwrap();

    if client_config.seperate_mode {

        let auth = Authentication::Manual {
            server_addr: SERVER_ADDR,
            client_id: 1,
            private_key: Key::default(),
            protocol_id: 0,
        };
       

        commands.entity(client).try_remove::<UdpIo>().try_remove::<SteamClientIo>().insert((
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

        commands.entity(client).try_remove::<UdpIo>().try_remove::<CrossbeamIo>().insert((
            NetcodeClient::new(auth, NetcodeConfig::default())?,
            SteamClientIo { target: ConnectTarget::Peer { steam_id: client_config.steam_connect_to.unwrap(), virtual_port: 4001 }, config: SessionConfig::default() },
            Link::new(None), // This is the link to the server, which will be established when the client connects
        ));



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
    commands.entity(client).try_remove::<CrossbeamIo>().try_remove::<SteamClientIo>().insert((
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