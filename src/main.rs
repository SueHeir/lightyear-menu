mod camera;
mod menu;
mod networking;

use std::sync::Arc;

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_simple_text_input::TextInputPlugin;

use bevy_tokio_tasks::tokio;
// use iyes_perf_ui::PerfUiPlugin;
use camera::CameraPlugin;
use lightyear::prelude::{SteamworksClient};
use menu::MenuPlugin;
use networking::{myserver::ExampleServerPlugin, new_headless_app, shared::SharedPlugin, NetworkingPlugin};
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

fn main() {
    //Test your server is working
    // let steam_client = Arc::new(RwLock::new(SteamworksClient::new_with_app_id(480)));
    // let mut app = new_headless_app();
    // app.add_plugins(PhysicsPlugins::default())
    //     .insert_resource(Gravity(Vec2::ZERO));


    // let game_state = GameState::Game;
    // app.insert_state(game_state);
    // let server_multiplayer_state = MultiplayerState::Server;
    // app.insert_state(server_multiplayer_state);

    // app.add_plugins(ExampleServerPlugin { predict_all: true, steam_client: steam_client.clone(), option_sender: None, option_reciever: None});


    // app.add_plugins(SharedPlugin);
    // app.run();



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
        .add_plugins(bevy_tokio_tasks::TokioTasksPlugin {
            make_runtime: Box::new(|| {
                let mut runtime = tokio::runtime::Builder::new_multi_thread();
                runtime.enable_all();
                runtime.build().unwrap()
            }),
            ..bevy_tokio_tasks::TokioTasksPlugin::default()
        })

        //Avian Physics
        .add_plugins(PhysicsPlugins::default().build())
        .insert_resource(Gravity(Vec2::ZERO))
        .add_plugins(PhysicsDebugPlugin::default())
        //Lightyear Setup
        .add_plugins(NetworkingPlugin)
        .insert_resource(client_config)
        //Menu Setup
        .init_state::<GameState>()
        .init_state::<MultiplayerState>()
        .add_plugins(MenuPlugin)
        .add_plugins(TextInputPlugin) //For IP Address Input
        //Game Setup
        .add_plugins(CameraPlugin)
        // .add_plugins(WorldInspectorPlugin::new())
        .run();
}

// Generic system that takes a component as a parameter, and will despawn all entities with that component
fn despawn_screen<T: Component>(to_despawn: Query<Entity, With<T>>, mut commands: Commands) {
    for entity in &to_despawn {
        commands.entity(entity).despawn_recursive();
    }
}
