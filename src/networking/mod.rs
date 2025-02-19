use std::sync::Arc;

use bevy::{
    app::{App, Plugin, Update}, ecs::{event::EventReader, schedule::IntoSystemConfigs, system::{Commands, Res, ResMut, Resource}}, input::{keyboard::KeyCode, ButtonInput}, state::{condition::in_state, state::{NextState, OnEnter, State}}
};
use client::ExampleClientPlugin;
use lightyear::{connection::client::ConnectionState, prelude::{client::{ClientCommandsExt, ClientConnection, ConnectedState}, server::{NetworkingState, ServerCommandsExt}, ClientDisconnectEvent, SteamworksClient}};
use parking_lot::RwLock;
use server::ExampleServerPlugin;

use crate::{GameState, MultiplayerState};

pub mod client;
pub mod protocol;
mod server;
mod shared;
use shared::SharedPlugin;



#[derive(Resource)]
pub struct SteamworksResource {
    pub steamworks: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>,
}

pub(crate) struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        // app.add_plugins(lightyear::client::ClientPlugin)

        let steam_client = Arc::new(RwLock::new(SteamworksClient::new_with_app_id(480)));

        app.insert_resource(SteamworksResource { steamworks: steam_client.clone() } );

        app.add_plugins(ExampleServerPlugin { predict_all: true, steam_client: steam_client.clone() });

        app.add_plugins(ExampleClientPlugin { steam_client: steam_client.clone() });

        // add our shared plugin containing the protocol + other shared behaviour
        app.add_plugins(SharedPlugin)
            .add_systems(OnEnter(MultiplayerState::Server), server::setup_server)
            .add_systems(OnEnter(MultiplayerState::Client), client::setup_client)
            .add_systems(OnEnter(MultiplayerState::HostServer), server::setup_server)
            .add_systems(
                OnEnter(NetworkingState::Started),
                client::setup_host_client.run_if(in_state(MultiplayerState::HostServer)),
            );


        app.add_systems(Update, (clean_up_game_on_client_disconnect, esc_to_disconnect));
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
) {

    if keys.just_pressed(KeyCode::Escape) {
        if MultiplayerState::Client == *multiplayer_state.get() {
            commands.disconnect_client();
            
        }

        if MultiplayerState::Server == *multiplayer_state.get() {
            commands.stop_server();
            game_state.set(GameState::Menu);
        }

        if MultiplayerState::HostServer == *multiplayer_state.get() {
            commands.stop_server();
        
            game_state.set(GameState::Menu);
        }
      
    }

    

}

