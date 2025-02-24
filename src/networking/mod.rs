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
use bevy_tokio_tasks::{tokio::task::JoinHandle, TokioTasksRuntime};
use myclient::ExampleClientPlugin;
use lightyear::{client::{config::{ClientConfig, NetcodeConfig}, networking}, prelude::{self, client::{Authentication, ClientCommandsExt, ClientTransport, IoConfig, NetConfig, NetworkingState}, server::{NetworkingState as ServerNetworkingState, ServerCommandsExt}, *}};
use lightyear::prelude::{client, server};
use lightyear::{inputs::leafwing::input_buffer::InputBuffer, prelude::*, shared::replication::components::Controlled, transport::LOCAL_SOCKET};
use parking_lot::RwLock;
use renderer::ExampleRendererPlugin;
use myserver::ExampleServerPlugin;
use tracing::{info, Level};

use bevy::prelude::*;

use crate::{GameState, MultiplayerState};

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
pub struct SpawnedServerHandler {
    pub handler: Option<JoinHandle<()>>
}

pub(crate) struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        //Create only one instance of the Steamworks client or else it will crash
        let steam_client = Arc::new(RwLock::new(SteamworksClient::new_with_app_id(480)));

        //resource is used to "resetup" client before connection when a steam players name is clicked
        app.insert_resource(SteamworksResource {
            steamworks: steam_client.clone(),
        });

        app.insert_resource(SpawnedServerHandler {
            handler: None,
        });

        //Steam netconfig is added when building the applocation.
        app.add_plugins(ExampleServerPlugin {
            predict_all: true,
            steam_client: steam_client.clone(),
            option_reciever: None,
            option_sender: None,
        });

        app.add_plugins(ExampleClientPlugin);

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
    mut game_state: ResMut<NextState<GameState>>,
    mut seperate_server: ResMut<SpawnedServerHandler>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        if MultiplayerState::Client == *multiplayer_state.get() {
            if let Some(server_handle) = &seperate_server.handler {
                server_handle.abort();
                commands.disconnect_client();
            } else {
                commands.disconnect_client();
            }
                          
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




/// App that is Send.
/// Used as a convenient workaround to send an App to a separate thread,
/// if we know that the App doesn't contain NonSend resources.
struct SendApp(App);

unsafe impl Send for SendApp {}
impl SendApp {
    fn run(&mut self) {
        self.0.run();
    }
}

pub fn new_headless_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            // Not strictly necessary, as the inclusion of ScheduleRunnerPlugin below
            // replaces the bevy_winit app runner and so a window is never created.
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            // WinitPlugin will panic in environments without a display server.
            .disable::<WinitPlugin>(),
    );

    // ScheduleRunnerPlugin provides an alternative to the default bevy_winit app runner, which
    // manages the loop without creating a window.
    app.add_plugins(ScheduleRunnerPlugin::run_loop(
            // Run 60 times per second.
            Duration::from_secs_f64(1.0 / 60.0),
        ));
    // app.add_plugins((
    //     MinimalPlugins,
    //     StatesPlugin,
    //     log_plugin(),
    //     HierarchyPlugin,
    //     DiagnosticsPlugin,
    // ));
    app
}

pub fn log_plugin() -> LogPlugin {
    LogPlugin {
        level: Level::INFO,
        filter: "wgpu=error,bevy_render=info,bevy_ecs=warn,bevy_time=warn".to_string(),
        ..Default::default()
    }
}


pub fn spawn_server_thread(
    runtime: ResMut<TokioTasksRuntime>, 
    steamworks: Res<SteamworksResource>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut client_setup_info: ResMut<crate::ClientConfigInfo>,
    mut client_config: ResMut<ClientConfig>,
    mut seperate_server: ResMut<SpawnedServerHandler>,
) {
   

    // we will communicate between the client and server apps via channels
    let (from_server_send, from_server_recv) = crossbeam_channel::unbounded();
    let (to_server_send, to_server_recv) = crossbeam_channel::unbounded();

    // create client app
    let io = IoConfig {
        // the address specified here is the client_address, because we open a UDP socket on the client
        transport: ClientTransport::LocalChannel { recv: from_server_recv, send: to_server_send },
        conditioner: None,
        compression: CompressionConfig::None,
     };

     let v4 = Ipv4Addr::from_str(&client_setup_info.address.as_str()).unwrap();
     let port = client_setup_info.port.parse::<u16>().unwrap();

     // Authentication is where you specify how the client should connect to the server
     // This is where you provide the server address.
     let auth = Authentication::Manual {
         server_addr: LOCAL_SOCKET,
         client_id: 0,
         private_key: Key::default(),
         protocol_id: 0,
     };

     let netcode_config = NetConfig::Netcode {  
         auth,
         io,
         config: NetcodeConfig {
             client_timeout_secs: 10,
             ..Default::default()
         },};
    
 
 
     client_config.net = netcode_config;



    let mut app = new_headless_app();
    app.add_plugins(PhysicsPlugins::default())
        .insert_resource(Gravity(Vec2::ZERO));


    let game_state = GameState::Game;
    app.insert_state(game_state);
    let server_multiplayer_state = MultiplayerState::Server;
    app.insert_state(server_multiplayer_state);

    app.add_plugins(ExampleServerPlugin { predict_all: true, steam_client: steamworks.steamworks.clone(), option_sender: Some(from_server_send), option_reciever: Some(to_server_recv)});


    app.add_plugins(SharedPlugin);


    let mut send_app = SendApp(app);
    // std::thread::spawn(move || send_app.run());
    let server = runtime.spawn_background_task(|_ctx| async move {
        send_app.run()
    });

    seperate_server.handler = Some(server);

    info!("Spawned Server as background task");

    client_setup_info.seperate_mode = true;

    multiplayer_state.set(MultiplayerState::Client);



}
