mod camera;
mod menu;
mod networking;

use std::{net::Ipv4Addr, str::FromStr, sync::{Arc, OnceLock}, time::Duration};
use parking_lot::{Mutex};
use avian2d::prelude::*;
use bevy::{app::ScheduleRunnerPlugin, gizmos::cross, log::{tracing_subscriber::Layer, BoxedLayer, LogPlugin}, prelude::*, winit::WinitPlugin};
use bevy_inspector_egui::{bevy_egui::EguiPlugin, quick::WorldInspectorPlugin};
use bevy_simple_text_input::TextInputPlugin;

// use iyes_perf_ui::PerfUiPlugin;
use camera::CameraPlugin;
use lightyear::{connection::prelude::server, prelude::{server::ServerPlugins, SteamId, SteamworksClient}, steam};
use lightyear::crossbeam::CrossbeamIo;
// use lightyear::{client::config::NetcodeConfig, prelude::{client::{Authentication, ClientTransport, IoConfig, NetConfig}, CompressionConfig, Key, SteamworksClient}, transport::LOCAL_SOCKET};
// use menu::MenuPlugin;
use networking::{server::ExampleServerPlugin, shared::SharedPlugin, NetworkingPlugin};
use clap::{Parser, Subcommand, ValueEnum};
use steamworks::{LobbyId, SingleClient};
use sync_cell::SyncCell;
use tracing::Level;

use crate::{menu::MenuPlugin, networking::shared::FIXED_TIMESTEP_HZ};


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
pub enum MultiplayerState {
    #[default]
    None,
    Server,
    Client,
    ClientSpawnServer,
}

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);

// Default setting for local testing (multiple instances on the same computer)
#[derive(Resource, Default)]
struct ClientConfigInfo {
    address: String,
    port: String,
    seperate_mode: bool,
    steam_connect_to: Option<(SteamId, LobbyId)>,
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



use tracing_appender::{non_blocking::WorkerGuard, rolling};


static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

fn custom_layer(_app: &mut App) -> Option<BoxedLayer> {
    let file_appender = rolling::daily("logs", "app.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);
    Some(bevy::log::tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_file(true)
            .with_line_number(true)
            .boxed())
}





/// CLI options to create an [`App`]
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub mode: Mode,
}

#[derive(Subcommand, Debug)]
pub enum Mode {
    Full,
    Client,
    Server,
}

// #[derive(Resource)]
// pub struct SteamSingleClient {
//     pub steam: SyncCell<lightyear::prelude::steamworks::SingleClient>,
// }






fn main() {
    
    
    let (crossbeam_client, crossbeam_server) = CrossbeamIo::new_pair();

    let (client_commands_send, client_commands_receive) = crossbeam_channel::unbounded::<ClientCommands>();
    let (server_commands_send, server_commands_receive) = crossbeam_channel::unbounded::<ServerCommands>();



    let mut server_app = new_headless_app();
    // app.add_plugins(PhysicsPlugins::default())
    //     .insert_resource(Gravity(Vec2::ZERO));


    let game_state = GameState::Menu;
    server_app.insert_state(game_state);
    let server_multiplayer_state = MultiplayerState::None;
    server_app.insert_state(server_multiplayer_state);


    //  let steam_client: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>> = Arc::new(RwLock::new(SteamworksClient::));
    let (steam_result) = lightyear::prelude::steamworks::Client::init_app(480);

    // let (steam, single_client) = lightyear::prelude::steamworks::SingleClient::init_app(480);


    let steam: Option<lightyear::prelude::steamworks::Client>;
    let wrapped_single_client: Option<Arc<Mutex<lightyear::prelude::steamworks::SingleClient>>>;
      
      
    if steam_result.is_err() {
        steam = None;
        wrapped_single_client = None;
    } else {
        let steam_tuple = steam_result.unwrap();
        steam = Some(steam_tuple.0);
        wrapped_single_client = Some(Arc::new(Mutex::new(steam_tuple.1)));
   

        // server_app.insert_resource(SteamworksClient(steam.clone().unwrap()));
        // server_app.insert_resource(resource);
        // server_app.add_systems(
        //     PreUpdate,
        //     |steam: ResMut<SteamSingleClient>| {
        //         steam.steam.borrow().run_callbacks();
        //     },);
    }
     

    server_app.add_plugins(ServerPlugins {
        tick_duration: Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ),
    });

    server_app.add_plugins(SharedPlugin);
    
    server_app.add_plugins(ExampleServerPlugin { 
        server_crossbeam: Some(crossbeam_server),
        client_recieve_commands:  Some(client_commands_receive),
        server_send_commands:  Some(server_commands_send),
        steam: steam.clone(),
        wrapped_single_client: wrapped_single_client.clone(),
    });


    let cli = Cli::parse();

    match cli.mode {
        Mode::Full => { //Client here does spawn server in background
            let mut send_app = SendApp(server_app);
            std::thread::spawn(move || send_app.run());
            info!("Spawned Server as background task (server is not started yet");
        },
        Mode::Client => {}, //Client here does not spawn server in background
        Mode::Server => {
            info!("Started Server as main task (server is auto started)");
            let game_state = GameState::Game;
            server_app.insert_state(game_state);
            let server_multiplayer_state = MultiplayerState::Server;
            server_app.insert_state(server_multiplayer_state);
            server_app.run();
            return;
        },
    }

  


    


    let client_config = ClientConfigInfo {
        address: "127.0.0.1".to_string(),
        port: "5000".to_string(),
        seperate_mode: false,
        steam_connect_to: None,
    };

    let mut client_app = App::new();


        //Bevy Setup
       client_app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Menu Example".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }).set(LogPlugin {
            // custom_layer,
            level: Level::INFO,
            // filter: "lightyear_netcode=trace,lightyear_crossbeam=trace".to_string(), //
            ..default() //
        }))
       

        //Avian Physics
        // .add_plugins(PhysicsPlugins::default().build())
        // .insert_resource(Gravity(Vec2::ZERO))
        // .add_plugins(PhysicsDebugPlugin::default())
        //Lightyear Setup
        .add_plugins(NetworkingPlugin { client_crossbeam: Some(crossbeam_client), 
            client_sender_commands: Some(client_commands_send.clone()),
            server_receive_commands: Some(server_commands_receive.clone()),
            steam: steam.clone(),
            wrapped_single_client: wrapped_single_client.clone(),
        });

        // if let Some((steamclient, steam_single)) = steam {
        //     info!("Steamworks client initialized successfully");
        //     client_app.insert_resource(lightyear::prelude::SteamworksClient(steamclient.clone()))
        //         .insert_non_send_resource(steam_single)
        //         .add_systems(
        //             PreUpdate,
        //             |steam: NonSend<lightyear::prelude::steamworks::SingleClient>| {
        //                 steam.run_callbacks();
        //             },
        //     );
        // } else {
        //     error!("Failed to initialize Steamworks client, running without Steam support");
        // }
        client_app
        .insert_resource(client_config)
        //Menu Setup
        .init_state::<GameState>()
        .init_state::<MultiplayerState>()
        .add_plugins(MenuPlugin)
        .add_plugins(TextInputPlugin) //For IP Address Input
        //Game Setup
        .add_plugins(CameraPlugin)
        // .add_systems(
        //     OnEnter(GameState::Menu),
        //     despawn_screen::<GameCleanUp>,
        // )
        .add_plugins(EguiPlugin { enable_multipass_for_primary_context: true })
        .add_plugins(WorldInspectorPlugin::new())
        .run();
}

// Generic system that takes a component as a parameter, and will despawn all entities with that component
fn despawn_screen<T: Component>(to_despawn: Query<Entity, With<T>>, mut commands: Commands) {
    for entity in &to_despawn {
        commands.entity(entity).despawn();
    }
}


pub fn new_headless_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            // .set(LogPlugin {
            //     // custom_layer,
            //     level: Level::DEBUG,
            //     filter: "lightyear_crossbeam=trace,lightyear_netcode=trace".to_string(), //
            //     ..default() //lightyear::client::prediction::rollback=debug,lightyear::server::prediction=debug
            // })
            .set(ImagePlugin::default_nearest())
            // Not strictly necessary, as the inclusion of ScheduleRunnerPlugin below
            // replaces the bevy_winit app runner and so a window is never created.
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            // WinitPlugin will panic in environments without a display server.
            .disable::<WinitPlugin>()
            // .disable::<LogPlugin>(),
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



