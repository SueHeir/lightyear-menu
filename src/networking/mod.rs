use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;

pub mod client;
pub mod server;
pub mod shared;
pub mod protocol;
pub mod renderer;
pub mod entity_label;

use client::ExampleClientPlugin;
use lightyear::prelude::client::ClientPlugins;

use lightyear::prelude::InterpolationRegistry;
use parking_lot::Mutex;
use shared::*;

use crate::networking::renderer::ExampleRendererPlugin;
use crate::ClientCommands;
use crate::ServerCommands;


pub(crate) struct NetworkingPlugin {
    pub client_crossbeam: Option<lightyear::crossbeam::CrossbeamIo>,
    pub client_sender_commands: Option<crossbeam_channel::Sender<ClientCommands>>,
    pub server_receive_commands: Option<crossbeam_channel::Receiver<ServerCommands>>,
    pub steam: Option<lightyear::prelude::steamworks::Client>,
    pub wrapped_single_client: Option<Arc<Mutex<lightyear::prelude::steamworks::SingleClient>>>,
}

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {

       
        // add lightyear plugins
        app.add_plugins(ClientPlugins {
            tick_duration: Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ),
        });

       

        app.add_plugins(SharedPlugin { show_confirmed: true});
        app.add_plugins(ExampleRendererPlugin);
        
        app.add_plugins(ExampleClientPlugin { client_crossbeam: self.client_crossbeam.clone(), 
            client_sender_commands: self.client_sender_commands.clone(),
            server_receive_commands: self.server_receive_commands.clone(),
            steam: self.steam.clone(),
            wrapped_single_client: self.wrapped_single_client.clone(),
        });


        
        
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