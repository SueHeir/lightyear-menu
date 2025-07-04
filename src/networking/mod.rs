use std::time::Duration;

use bevy::prelude::*;

pub mod client;
pub mod server;
pub mod shared;

use client::ExampleClientPlugin;
use lightyear::prelude::client::ClientPlugins;

use shared::*;



pub(crate) struct NetworkingPlugin {
    // pub(crate) steam_client: Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, SteamworksClient>>,
    // pub(crate) client_config: NetConfig,
    // pub(crate) client_commands_send: Sender<ClientCommands>,
    // pub(crate) server_commands_receive: Receiver<ServerCommands>,
}

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        // add lightyear plugins
        app.add_plugins(ClientPlugins {
            tick_duration: Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ),
        });

        app.add_plugins(SharedPlugin);
        
        app.add_plugins(ExampleClientPlugin);
        
    }
}

// pub fn clean_up_game_on_client_disconnect(
//     mut disconnect_event: EventReader<ClientDisconnectEvent>,
//     mut game_state: ResMut<NextState<GameState>>,
//     mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
// ) {
//     for event in disconnect_event.read() {
//         println!("{:?}", event.reason);

//         game_state.set(GameState::Menu);
//         multiplayer_state.set(MultiplayerState::None);
//     }
// }

// pub fn esc_to_disconnect(
//     keys: Res<ButtonInput<KeyCode>>,
//     mut commands: Commands,
//     multiplayer_state: Res<State<MultiplayerState>>,
//     client_commands: Res<ClientCommandsSender>,
//     mut game_state: ResMut<NextState<GameState>>,
// ) {
//     if keys.just_pressed(KeyCode::Escape) {
//         if MultiplayerState::Client == *multiplayer_state.get() {
           
//             commands.disconnect_client();
//             let result =  client_commands.client_commands.send(ClientCommands::StopServer);

//             info!("{:?}", result);
            
                          
//         }

//         if MultiplayerState::Server == *multiplayer_state.get() {
//             commands.stop_server();
//             game_state.set(GameState::Menu); //MultiplayerState is set to None OnEnter(Menu) in menu.rs
//         }

//         if MultiplayerState::HostServer == *multiplayer_state.get() {
//             commands.stop_server();

//             game_state.set(GameState::Menu); //MultiplayerState is set to None OnEnter(Menu) in menu.rs
//         }
//     }
// }



// pub fn spawn_server_thread(
//     mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
//     mut client_setup_info: ResMut<crate::ClientConfigInfo>,
//     mut client_config: ResMut<ClientConfig>,
//     seperate_client_config: Res<SeperateClientConfig>,

//     mut client_commands: ResMut<ClientCommandsSender>,

// ) {
   
//     let result =  client_commands.client_commands.send(ClientCommands::StartServer);

//     info!("{:?}", result);

//     client_config.net = seperate_client_config.client_config.clone();

//     client_setup_info.seperate_mode = true;
// }