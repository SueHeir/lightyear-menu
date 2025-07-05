//! This module contains the shared code between the client and the server.

use bevy::prelude::*;
use crossbeam_channel::{Receiver, TryRecvError};
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use core::time::Duration;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

pub const FIXED_TIMESTEP_HZ: f64 = 64.0;

pub const SERVER_REPLICATION_INTERVAL: Duration = Duration::from_millis(100);

pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);

#[derive(Clone)]
pub struct SharedPlugin;

pub struct Channel1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message1(pub usize);

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        // Register your protocol, which is shared between client and server
        app.add_message::<Message1>()
            .add_direction(NetworkDirection::Bidirectional);

        app.add_channel::<Channel1>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        });
    }
}







#[derive(Resource)]
struct CrossbeamEventReceiver<T: Event>(Receiver<T>);

pub trait CrossbeamEventApp {
    fn add_crossbeam_event<T: Event>(&mut self, receiver: Receiver<T>) -> &mut Self;
}

impl CrossbeamEventApp for App {
    fn add_crossbeam_event<T: Event>(&mut self, receiver: Receiver<T>) -> &mut Self {
        self.insert_resource(CrossbeamEventReceiver::<T>(receiver));
        self.add_event::<T>();
        self.add_systems(PreUpdate, process_crossbeam_messages::<T>);
        self
    }
}


fn process_crossbeam_messages<T: Event>(
    receiver: Res<CrossbeamEventReceiver<T>>,
    mut events: EventWriter<T>,
) {
    loop {
        match receiver.0.try_recv() {
            Ok(msg) => {
                events.write(msg);
            }
            Err(TryRecvError::Disconnected) => {
                panic!("sender resource dropped")
            }
            Err(TryRecvError::Empty) => {
                break;
            }
        }
    }
}