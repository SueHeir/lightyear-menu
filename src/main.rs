
mod menu;
mod networking;
mod camera;

use std::env;

use avian2d::{math::*, prelude::*};
use bevy::{color::palettes, prelude::*, window::{WindowMode, WindowResolution}, winit::WinitSettings};


use bevy_inspector_egui::{
    quick::{ResourceInspectorPlugin, WorldInspectorPlugin},
    InspectorOptions,
};
use bevy_simple_text_input::TextInputPlugin;

// use iyes_perf_ui::PerfUiPlugin;
use menu::MenuPlugin;
use networking::NetworkingPlugin;
use camera::CameraPlugin;
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
}

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);

// `InspectorOptions` are completely optional
#[derive(Resource, Default, InspectorOptions)]
struct ClientConfigInfo {
    address: String,
    port: String,
    local_testing: bool,
    steam_testing: bool,
    steam_connect_to: Option<SteamId>,
}

fn main() {

    let client_config = ClientConfigInfo {
        address: "127.0.0.1".to_string(),
        port: "5000".to_string(),
        local_testing: true, //Change this to false for release build
        steam_testing: false,
        steam_connect_to: None,
    };

    App::new()
        //Bevy Setup
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Menu Example".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
        )
        
        //Avian Physics
        .add_plugins(
            PhysicsPlugins::default()
                // .with_length_unit(100.0)
                .build(),
        )
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
