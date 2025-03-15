mod camera;
mod menu;
mod networking;

use std::{net::Ipv4Addr, str::FromStr, sync::Arc, time::Duration};

use avian2d::prelude::*;
use bevy::{app::ScheduleRunnerPlugin, prelude::*, winit::WinitPlugin};
use bevy_simple_text_input::TextInputPlugin;

// use iyes_perf_ui::PerfUiPlugin;
use camera::CameraPlugin;
use lightyear::{client::config::NetcodeConfig, prelude::{client::{Authentication, ClientTransport, IoConfig, NetConfig}, CompressionConfig, Key, SteamworksClient}, transport::LOCAL_SOCKET};
use menu::MenuPlugin;
use networking::{myserver::ExampleServerPlugin, shared::SharedPlugin, NetworkingPlugin};
use parking_lot::RwLock;
use steamworks::SteamId;


#[derive(Component)]
pub struct GameCleanUp;
// Enum that will be used as a global state for the game
#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, States)]
enum GameState {
    #[default]
    Menu,
    Game,
}

// Enum that will be used as a global state for the games multiplayer setup
#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, States)]
enum MultiplayerState {
    #[default]
    None,
    Server,
    Client,
    HostServer,
    ClientSpawnServer,
}

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);

// Default setting for local testing (multiple instances on the same computer)
#[derive(Resource, Default)]
struct ClientConfigInfo {
    address: String,
    port: String,
    local_testing: bool,
    seperate_mode: bool,
    steam_testing: bool,
    steam_connect_to: Option<SteamId>,
}



#[derive(Event)]
pub enum ClientCommands {
    StartServer,
    StopServer,
}


#[derive(Event)]
pub enum ServerCommands {
    ServerStarted,
}


fn main() {
    
    // we will communicate between the client and server apps via channels
    let (from_server_send, from_server_recv) = crossbeam_channel::unbounded();
    let (to_server_send, to_server_recv) = crossbeam_channel::unbounded();
    let (client_commands_send, client_commands_receive) = crossbeam_channel::unbounded::<ClientCommands>();
    let (server_commands_send, server_commands_receive) = crossbeam_channel::unbounded::<ServerCommands>();

    // create client app
    let io = IoConfig {
        // the address specified here is the client_address, because we open a UDP socket on the client
        transport: ClientTransport::LocalChannel { recv: from_server_recv, send: to_server_send },
        conditioner: None,
        compression: CompressionConfig::None,
     };

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



    let mut app = new_headless_app();
    app.add_plugins(PhysicsPlugins::default())
        .insert_resource(Gravity(Vec2::ZERO));


    let game_state = GameState::Menu;
    app.insert_state(game_state);
    let server_multiplayer_state = MultiplayerState::None;
    app.insert_state(server_multiplayer_state);


    let steam_client: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>> = Arc::new(RwLock::new(SteamworksClient::new_with_app_id(480).unwrap()));

    app.add_plugins(ExampleServerPlugin { 
        predict_all: true, 
        steam_client: steam_client.clone(), 
        option_sender: Some(from_server_send), 
        option_reciever: Some(to_server_recv), 
        client_recieve_commands: Some(client_commands_receive.clone()),
        server_send_commands: server_commands_send.clone(),
    });


    app.add_plugins(SharedPlugin)
        .add_systems(
            OnEnter(GameState::Menu),
            (despawn_screen::<GameCleanUp>),
        );


    let mut send_app = SendApp(app);
    std::thread::spawn(move || send_app.run());


    info!("Spawned Server as background task");


    let client_config = ClientConfigInfo {
        address: "127.0.0.1".to_string(),
        port: "5000".to_string(),
        local_testing: true, //Change this to false for testing across multiple computers
        seperate_mode: false,
        steam_testing: false,
        steam_connect_to: None,
    };

    App::new()
        //Bevy Setup
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Menu Example".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }))

        //Avian Physics
        .add_plugins(PhysicsPlugins::default().build())
        .insert_resource(Gravity(Vec2::ZERO))
        .add_plugins(PhysicsDebugPlugin::default())
        //Lightyear Setup
        .add_plugins(NetworkingPlugin { steam_client: steam_client.clone(), client_config: netcode_config, client_commands_send: client_commands_send, server_commands_receive: server_commands_receive.clone() })
        .insert_resource(client_config)
        //Menu Setup
        .init_state::<GameState>()
        .init_state::<MultiplayerState>()
        .add_plugins(MenuPlugin)
        .add_plugins(TextInputPlugin) //For IP Address Input
        //Game Setup
        .add_plugins(CameraPlugin)
        .add_systems(
            OnEnter(GameState::Menu),
            (despawn_screen::<GameCleanUp>),
        )
        // .add_plugins(WorldInspectorPlugin::new())
        .run();
}

// Generic system that takes a component as a parameter, and will despawn all entities with that component
fn despawn_screen<T: Component>(to_despawn: Query<Entity, With<T>>, mut commands: Commands) {
    for entity in &to_despawn {
        commands.entity(entity).despawn_recursive();
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

