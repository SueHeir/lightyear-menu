//! The client plugin.
use crate::networking::shared::*;
use crate::MultiplayerState;
use bevy::prelude::*;
use core::net::Ipv4Addr;
use core::net::{IpAddr, SocketAddr};
use lightyear::netcode::Key;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

pub struct ExampleClientPlugin;

const CLIENT_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4000);

impl Plugin for ExampleClientPlugin {
    fn build(&self, app: &mut App) {
        // add our client-specific logic. Here we will just connect to the server
        app.add_systems(Startup, spawn_client);
        app.add_systems(OnEnter(MultiplayerState::ClientSpawnServer), client_spawn_server);
        app.add_systems(OnEnter(MultiplayerState::Client), client_connect);

    }
}
fn client_spawn_server(mut commands: Commands) {

}



/// Spawn a client that connects to the server
fn spawn_client(mut commands: Commands) -> Result {
    let auth = Authentication::Manual {
        server_addr: SERVER_ADDR,
        client_id: 0,
        private_key: Key::default(),
        protocol_id: 0,
    };
    commands
        .spawn((
            Client::default(),
            LocalAddr(CLIENT_ADDR),
            PeerAddr(SERVER_ADDR),
            Link::new(None),
            ReplicationReceiver::default(),
            NetcodeClient::new(auth, NetcodeConfig::default())?,
            UdpIo::default(),
        ));
    Ok(())
}

/// Spawn a client that connects to the server
fn client_connect(mut commands: Commands, client_q: Query<Entity, With<Client>>) -> Result {
    
    let client = client_q.single_inner().ok().unwrap();
    commands.trigger_targets(Connect, client);
    Ok(())
}
