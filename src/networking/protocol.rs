use std::time::Duration;

use avian2d::prelude::*;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use serde::{Deserialize, Serialize};

use lightyear::client::components::{ComponentSyncMode, LerpFn};
use lightyear::prelude::*;
use lightyear::utils::avian2d::*;
use lightyear::utils::bevy::TransformLinearInterpolation;


// For prediction, we want everything entity that is predicted to be part of the same replication group
// This will make sure that they will be replicated in the same message and that all the entities in the group
// will always be consistent (= on the same tick)
pub const REPLICATION_GROUP: ReplicationGroup = ReplicationGroup::new_id(1);


// Components
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct PlayerNetworkInfo {
    pub client_id: ClientId,
    pub nickname: String,
    pub rtt: Duration,
    pub jitter: Duration,
}

impl PlayerNetworkInfo {
    pub fn new(client_id: ClientId, nickname: String) -> Self {
        Self {
            client_id,
            nickname,
            rtt: Duration::ZERO,
            jitter: Duration::ZERO,
        }
    }
}

// Channels

#[derive(Channel)]
pub struct Channel1;

// Messages

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message1(pub usize);

// Inputs

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Hash, Reflect)]
pub enum PlayerActions {
    Move,
    RespawnRequest,
}

impl Actionlike for PlayerActions {
    fn input_control_kind(&self) -> InputControlKind {
        match self {
            Self::Move => InputControlKind::DualAxis,
            _ => InputControlKind::Button,
        }
    }
}





// Limiting firing rate: once you fire on `last_fire_tick` you have to wait `cooldown` ticks before firing again.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ActionTracker {
    pub action1_last_tick: Tick,
    pub action2_last_tick: Tick,
    pub action1_cooldown: u16,
    pub action2_cooldown: u16,
    pub action1_max_channel: u16,
    pub action2_max_channel: u16,
}

impl ActionTracker {
    pub(crate) fn new(cooldown: (u16, u16), max_channel: (u16, u16)) -> Self {
        Self {
            action1_last_tick: Tick(0),
            action2_last_tick: Tick(0),
            action1_cooldown: cooldown.0,
            action2_cooldown: cooldown.1,
            action1_max_channel: max_channel.0,
            action2_max_channel: max_channel.1,
        }
    }
}

// despawns `lifetime` ticks after `origin_tick`
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Lifetime {
    pub(crate) origin_tick: Tick,
    /// number of ticks to live for
    pub(crate) lifetime: i16,
}



// Protocol
pub(crate) struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        // inputs
        app.add_plugins(LeafwingInputPlugin::<PlayerActions>::default());
        // components

        // Player is synced as Simple, because we periodically update rtt ping stats
        app.register_component::<PlayerNetworkInfo>(ChannelDirection::Bidirectional)
            .add_prediction(ComponentSyncMode::Simple);


        app.register_component::<Lifetime>(ChannelDirection::Bidirectional)
            .add_prediction(ComponentSyncMode::Once);

        app.register_component::<ActionTracker>(ChannelDirection::Bidirectional)
            .add_prediction(ComponentSyncMode::Full);

        // NOTE: interpolation/correction is only needed for components that are visually displayed!
        // we still need prediction to be able to correctly predict the physics on the client
        app.register_component::<LinearVelocity>(ChannelDirection::Bidirectional)
            .add_prediction(ComponentSyncMode::Full);

        app.register_component::<Position>(ChannelDirection::Bidirectional)
            .add_prediction(ComponentSyncMode::Full)
            .add_interpolation(ComponentSyncMode::Full)
            .add_interpolation_fn(position::lerp)
            .add_correction_fn(position::lerp);

        app.register_component::<Rotation>(ChannelDirection::Bidirectional)
            .add_prediction(ComponentSyncMode::Full)
            .add_interpolation_fn(rotation::lerp)
            .add_correction_fn(rotation::lerp);

        // do not replicate Transform but make sure to register an interpolation function
        // for it so that we can do visual interpolation
        // (another option would be to replicate transform and not use Position/Rotation at all)
        app.add_interpolation::<Transform>(ComponentSyncMode::None);
        app.add_interpolation_fn::<Transform>(TransformLinearInterpolation::lerp);

        // channels
        app.add_channel::<Channel1>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        });
    }
}
