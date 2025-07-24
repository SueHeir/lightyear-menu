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
       
        
        app.add_plugins(ExampleClientPlugin { client_crossbeam: self.client_crossbeam.clone(), 
            client_sender_commands: self.client_sender_commands.clone(),
            server_receive_commands: self.server_receive_commands.clone(),
            steam: self.steam.clone(),
            wrapped_single_client: self.wrapped_single_client.clone(),
        });


         app.add_plugins(ExampleRendererPlugin);
         
    }
}
