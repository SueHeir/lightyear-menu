//! The server side of the example.
//! It is possible (and recommended) to run the server in headless mode (without any rendering plugins).
//!
//! The server will:
//! - spawn a new player entity for each client that connects
//! - read inputs from the clients and move the player entities accordingly
//!
//! Lightyear will handle the replication of entities automatically if you add a `Replicate` component to them.
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::time::Duration;

use crate::networking::shared::*;
use crate::ClientCommands;
use crate::GameState;
use crate::ServerCommands;
use bevy::prelude::*;
use lightyear::crossbeam::CrossbeamIo;
use crossbeam_channel::Sender;
use crossbeam_channel::Receiver;
use lightyear::link::LinkSender;
use lightyear::prelude::server::*;
use lightyear::prelude::steamworks::SingleClient;
use lightyear::prelude::*;
use crate::MultiplayerState;

#[derive(Resource)]
pub struct ServerCommandSender {
    pub server_commands: Sender<ServerCommands>,
}

#[derive(Resource)]
pub struct ServerStartupResources{
    pub server_crossbeam: Option<CrossbeamIo>,
}


pub struct ExampleServerPlugin {
    pub server_crossbeam: Option<CrossbeamIo>,
    pub client_recieve_commands:   Option<Receiver<ClientCommands>>,
    pub server_send_commands:  Option<Sender<ServerCommands>>,
}

impl Plugin for ExampleServerPlugin {
    fn build(&self, app: &mut App) {

        // Create the server immediately
        let server_entity = app.world_mut().spawn((
            NetcodeServer::new(NetcodeConfig::default()),
            LocalAddr(SERVER_ADDR),
            ServerUdpIo::default(),
        )).id();

         if let Some(server_crossbeam) = &self.server_crossbeam {
            // You need to provide a valid client_id here. For demonstration, we'll use 12345.
            info!("Add a Linked connection for host client to server");
            
            let mut entity = app.world_mut().spawn(LinkOf {
                server: server_entity,
            });
            entity.insert(PingManager::new(PingConfig {
                ping_interval: Duration::default(),
            }));
            entity.insert(Link::new(None));
            entity.insert(Linked);
            entity.insert(server_crossbeam.clone());
    //        LinkOf {
    //             server: self.server_entity,
    //         },
    //         // Send pings every frame, so that the Acks are sent every frame
    //         PingManager::new(PingConfig {
    //             ping_interval: Duration::default(),
    //         }),
    //         // TODO: we want the ReplicationSender/Receiver to be added automatically when ClientOf is created, but with configs pre-specified by the server
    //         ReplicationSender::default(),
    //         ReplicationReceiver::default(),
    //         // we will act like each client has a different port
    //         Link::new(None),
    //         PeerAddr(SocketAddr::new(
    //             core::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
    //             client_id as u16,
    //         )),
    //         // For Crossbeam we need to mark the IO as Linked, as there is no ServerLink to do that for us
    //         Linked,
    //         crossbeam_server,
    //         TestHelper::default(),
        } 

        app.insert_resource(ServerStartupResources {
            server_crossbeam: self.server_crossbeam.clone(),
        });


        



        if self.client_recieve_commands.is_some() {
            app.add_crossbeam_event(self.client_recieve_commands.clone().unwrap().clone());
            app.add_observer(handle_server_started);
        }
        if self.server_send_commands.is_some() {
            app.insert_resource(ServerCommandSender { server_commands: self.server_send_commands.clone().unwrap().clone() });
            app.add_systems(FixedUpdate, handle_client_commands);
        }
       
        // app.add_systems(OnEnter(GameState::Game), init.run_if(in_state(MultiplayerState::Server).or(in_state(MultiplayerState::HostServer))));
        app.add_systems(OnEnter(MultiplayerState::Server), start_server);
        app.add_observer(handle_new_client);
    }
}

/// Whenever a new client connects to the server, a new entity will get spawned with
/// the `Connected` component, which represents the connection between the server and that specific client.
///
/// You can add more components to customize how this connection, for example by adding a
/// `ReplicationSender` (so that the server can send replication updates to that client)
/// or a `MessageSender`.
fn handle_new_client(trigger: Trigger<OnAdd, Connected>, mut commands: Commands) {
    info!("Handle new client");
    commands
        .entity(trigger.target())
        .insert(ReplicationSender::new(
            SERVER_REPLICATION_INTERVAL,
            SendUpdatesMode::SinceLastAck,
            false,
        ));
   
}


pub fn start_server(mut commands: Commands, server_q: Query<Entity, With<Server>>) {

    if let Some(server) = server_q.iter().next() {
        commands.trigger_targets(Start, server);
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
    let _ = server_commands.server_commands.send(ServerCommands::ServerStarted);
}


pub(crate) fn handle_client_commands(
    mut client_commands: EventReader<ClientCommands>,
    mut commands: Commands,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut game_state: ResMut<NextState<GameState>>,
    mut server_q: Query<Entity, With<Server>>
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
                 if let Some(server) = server_q.iter().next() {
                    commands.trigger_targets(Stop, server);
                    info!("Server Stopped");
                }
                multiplayer_state.set(MultiplayerState::None);
                game_state.set(GameState::Menu);


            },
        }
    }
}
