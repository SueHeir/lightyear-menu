use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, ops::Mul, str::FromStr, sync::Arc, thread::sleep, time::Duration};

use avian2d::{prelude::Gravity, PhysicsPlugins};
use bevy::{
    app::{App, Plugin, ScheduleRunnerPlugin, Update}, diagnostic::DiagnosticsPlugin, ecs::{
        event::EventReader,
        schedule::IntoSystemConfigs,
        system::{Commands, Res, ResMut, Resource},
    }, hierarchy::HierarchyPlugin, input::{keyboard::KeyCode, ButtonInput}, log::LogPlugin, math::Vec2, render::texture::ImagePlugin, state::{
        app::{AppExtStates, StatesPlugin}, condition::in_state, state::{NextState, OnEnter, State}
    }, window::WindowPlugin, winit::WinitPlugin, MinimalPlugins
};
use crossbeam_channel::Sender;
use myclient::{ClientCommandsSender, ExampleClientPlugin};
use lightyear::{client::{config::{ClientConfig, NetcodeConfig}, networking}, prelude::{self, client::{Authentication, ClientCommandsExt, ClientTransport, IoConfig, NetConfig, NetworkingState}, server::{NetworkingState as ServerNetworkingState, ServerCommandsExt}, *}};
use lightyear::prelude::{client, server};
use lightyear::{inputs::leafwing::input_buffer::InputBuffer, prelude::*, shared::replication::components::Controlled, transport::LOCAL_SOCKET};
use parking_lot::RwLock;
use renderer::ExampleRendererPlugin;
use myserver::ExampleServerPlugin;
use tracing::{info, Level};

use bevy::prelude::*;

use crate::{ClientCommands, GameState, MultiplayerState};

pub mod myclient;
pub mod protocol;
mod renderer;
pub mod myserver;
pub mod shared;
pub mod entity_label;
use shared::SharedPlugin;

#[derive(Resource)]
pub struct SteamworksResource {
    pub steamworks: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>,
}

#[derive(Resource)]
pub struct SeperateClientConfig {
    pub client_config: NetConfig,
}


pub(crate) struct NetworkingPlugin {
    pub(crate) steam_client: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>,
    pub(crate) client_config: NetConfig,
    pub(crate) client_commands_send: Sender<ClientCommands>,
}

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        //Create only one instance of the Steamworks client or else it will crash
        

        //resource is used to "resetup" client before connection when a steam players name is clicked
        app.insert_resource(SteamworksResource {
            steamworks: self.steam_client.clone(),
        });

        app.insert_resource(SeperateClientConfig {
           client_config: self.client_config.clone(),
        });


        // //Steam netconfig is added when building the application.
        // app.add_plugins(ExampleServerPlugin {
        //     predict_all: true,
        //     steam_client: self.steam_client.clone(),
        //     option_reciever: None,
        //     option_sender: None,
        //     client_recieve_commands: None,
        // });

        app.add_plugins(ExampleClientPlugin {client_commands: self.client_commands_send.clone()});

        // add our shared plugin containing the protocol and renderer
        app.add_plugins(SharedPlugin);
        app.add_plugins(ExampleRendererPlugin { show_confirmed: false });

        app.add_systems(OnEnter(MultiplayerState::Client), myclient::setup_client) // Starts the client with information in ClientConfigInfo (see main.rs and menu.rs)
            .add_systems(OnEnter(MultiplayerState::HostServer), myserver::setup_server)
            .add_systems(
                OnEnter(ServerNetworkingState::Started),
                myclient::setup_host_client.run_if(in_state(MultiplayerState::HostServer)),
            ) //Waits until server is started to start the client
            .add_systems(OnEnter(MultiplayerState::ClientSpawnServer), spawn_server_thread);

        //Pressing escape will bring you to main menu, if you are disconnected it will also bring you to the main menu
        app.add_systems(
            Update,
            (clean_up_game_on_client_disconnect, esc_to_disconnect),
        );
    }
}

pub fn clean_up_game_on_client_disconnect(
    mut disconnect_event: EventReader<ClientDisconnectEvent>,
    mut game_state: ResMut<NextState<GameState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
) {
    for event in disconnect_event.read() {
        println!("{:?}", event.reason);

        game_state.set(GameState::Menu);
        multiplayer_state.set(MultiplayerState::None);
    }
}

pub fn esc_to_disconnect(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    multiplayer_state: Res<State<MultiplayerState>>,
    client_commands: Res<ClientCommandsSender>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        if MultiplayerState::Client == *multiplayer_state.get() {
           
            commands.disconnect_client();
            let result =  client_commands.client_commands.send(ClientCommands::StopServer);

            info!("{:?}", result);
            
                          
        }

        if MultiplayerState::Server == *multiplayer_state.get() {
            commands.stop_server();
            game_state.set(GameState::Menu); //MultiplayerState is set to None OnEnter(Menu) in menu.rs
        }

        if MultiplayerState::HostServer == *multiplayer_state.get() {
            commands.stop_server();

            game_state.set(GameState::Menu); //MultiplayerState is set to None OnEnter(Menu) in menu.rs
        }
    }
}



pub fn spawn_server_thread(
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut client_setup_info: ResMut<crate::ClientConfigInfo>,
    mut client_config: ResMut<ClientConfig>,
    seperate_client_config: Res<SeperateClientConfig>,

    mut client_commands: ResMut<ClientCommandsSender>,

) {
   
    let result =  client_commands.client_commands.send(ClientCommands::StartServer);

    info!("{:?}", result);

    client_config.net = seperate_client_config.client_config.clone();

    client_setup_info.seperate_mode = true;

    sleep(Duration::from_millis(1000));

    multiplayer_state.set(MultiplayerState::Client);



}
