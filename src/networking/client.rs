//! The client plugin.
use crate::{GameCleanUp, MultiplayerState};

use super::{
    protocol::{BallMarker, ColorComponent, PhysicsBundle, PlayerActions, PlayerBundle, PlayerId},
    shared::{shared_config, shared_movement_behaviour},
    SteamworksResource,
};
use avian2d::prelude::{LinearVelocity, Position};
use bevy::prelude::*;

use leafwing_input_manager::prelude::{ActionState, InputMap};
pub use lightyear::prelude::client::*;
use lightyear::{prelude::*, transport::LOCAL_SOCKET};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
    time::Duration,
};
pub struct ExampleClientPlugin;

/// Here we create the lightyear [`ClientPlugins`]
fn build_host_client_plugin() -> ClientPlugins {
    // Authentication is where you specify how the client should connect to the server
    // This is where you provide the server address.
    let net_config = NetConfig::Local { id: 0 };
    let mut config = ClientConfig {
        // part of the config needs to be shared between the client and server
        shared: shared_config(),
        net: net_config,
        ..default()
    };

    config.prediction.set_fixed_input_delay_ticks(10);
    config.prediction.correction_ticks_factor = 1.5;
    config.prediction.maximum_predicted_ticks = 100;
    ClientPlugins::new(config)
}

impl Plugin for ExampleClientPlugin {
    fn build(&self, app: &mut App) {
        // add lightyear plugins
        app.add_plugins(build_host_client_plugin());

        app.add_systems(
            PreUpdate,
            handle_connection
                .after(MainSet::Receive)
                .before(PredictionSet::SpawnPrediction),
        );
        // all actions related-system that can be rolled back should be in FixedUpdate schedule
        app.add_systems(
            FixedUpdate,
            player_movement.run_if(
                in_state(MultiplayerState::Client).or(in_state(MultiplayerState::HostServer)),
            ),
        ); //We want hostservers to run this
        app.add_systems(
            Update,
            (
                add_ball_physics.run_if(in_state(MultiplayerState::Client)),
                add_player_physics.run_if(in_state(MultiplayerState::Client)),
                // send_message,
                handle_predicted_spawn.run_if(in_state(MultiplayerState::Client)),
                handle_interpolated_spawn.run_if(in_state(MultiplayerState::Client)),
            ),
        );
    }
}

pub fn setup_client(
    mut commands: Commands,
    mut client_config: ResMut<ClientConfig>,
    client_setup_info: Res<crate::ClientConfigInfo>,
    mut steamworks: ResMut<SteamworksResource>,
) {
    if client_setup_info.steam_testing {
        if let Some(id) = client_setup_info.steam_connect_to {
            let net_config = NetConfig::Steam {
                steamworks_client: Some(steamworks.steamworks.clone()),
                config: SteamConfig {
                    socket_config: SocketConfig::P2P {
                        virtual_port: 5002,
                        steam_id: id.raw(),
                    },
                    app_id: 480,
                },
                conditioner: None,
            };

            client_config.net = net_config;

            client_config.prediction.set_fixed_input_delay_ticks(10);
            client_config.prediction.correction_ticks_factor = 1.5;
            client_config.prediction.maximum_predicted_ticks = 100;

            // Connect to the server
            commands.connect_client();
        }

        return;
    }

    let v4 = Ipv4Addr::from_str(&client_setup_info.address.as_str()).unwrap();
    let port = client_setup_info.port.parse::<u16>().unwrap();

    let server_addr = SocketAddr::new(IpAddr::V4(v4), port);

    // Authentication is where you specify how the client should connect to the server
    // This is where you provide the server address.
    let auth = Authentication::Manual {
        server_addr: server_addr,
        client_id: rand::random::<u64>(),
        private_key: Key::default(),
        protocol_id: 0,
    };

    let transport;
    if client_setup_info.local_testing {
        transport = ClientTransport::UdpSocket(LOCAL_SOCKET);
    } else {
        transport = ClientTransport::UdpSocket(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            port,
        ));
    }

    // The IoConfig will specify the transport to use.
    let io = IoConfig {
        // the address specified here is the client_address, because we open a UDP socket on the client
        transport: transport,
        conditioner: Some(LinkConditionerConfig {
            incoming_latency: Duration::from_millis(100),
            incoming_jitter: Duration::from_millis(30),
            incoming_loss: 0.001,
        }),
        ..default()
    };
    // The NetConfig specifies how we establish a connection with the server.
    // We can use either Steam (in which case we will use steam sockets and there is no need to specify
    // our own io) or Netcode (in which case we need to specify our own io).
    let net_config = NetConfig::Netcode {
        auth,
        io,
        config: NetcodeConfig {
            client_timeout_secs: 4,
            ..Default::default()
        },
    };

    client_config.net = net_config;

    client_config.prediction.set_fixed_input_delay_ticks(10);
    client_config.prediction.correction_ticks_factor = 1.5;
    client_config.prediction.maximum_predicted_ticks = 100;

    // Connect to the server
    commands.connect_client();

    println!("trying to connect")
}

pub fn setup_host_client(mut commands: Commands, mut client_config: ResMut<ClientConfig>) {
    // The NetConfig specifies how we establish a connection with the server.
    // We can use either Steam (in which case we will use steam sockets and there is no need to specify
    // our own io) or Netcode (in which case we need to specify our own io).
    let net_config = NetConfig::Local { id: 0 };

    client_config.net = net_config;

    client_config.prediction.set_fixed_input_delay_ticks(6);
    client_config.prediction.correction_ticks_factor = 1.5;
    client_config.prediction.maximum_predicted_ticks = 100;
    // Connect to the server
    commands.connect_client();

    println!("trying to connect")
}

/// Listen for events to know when the client is connected, and spawn player entities
pub(crate) fn handle_connection(
    mut commands: Commands,
    mut connection_event: EventReader<ConnectEvent>,
) {
    for event in connection_event.read() {
        let client_id = event.client_id();
        let y = (client_id.to_bits() as f32 * 50.0) % 500.0 - 250.0;
        // we will spawn two cubes per player, once is controlled with WASD, the other with arrows
        commands.spawn((
            PlayerBundle::new(
                client_id,
                Vec2::new(-50.0, y),
                InputMap::new([
                    (PlayerActions::Up, KeyCode::KeyW),
                    (PlayerActions::Down, KeyCode::KeyS),
                    (PlayerActions::Left, KeyCode::KeyA),
                    (PlayerActions::Right, KeyCode::KeyD),
                ]),
            ),
            GameCleanUp,
        ));
        // commands.spawn((PlayerBundle::new(
        //     client_id,
        //     Vec2::new(50.0, y),
        //     InputMap::new([
        //         (PlayerActions::Up, KeyCode::ArrowUp),
        //         (PlayerActions::Down, KeyCode::ArrowDown),
        //         (PlayerActions::Left, KeyCode::ArrowLeft),
        //         (PlayerActions::Right, KeyCode::ArrowRight),
        //     ]),
        // ),));
    }
}

/// Blueprint pattern: when the ball gets replicated from the server, add all the components
/// that we need that are not replicated.
/// (for example physical properties that are constant, so they don't need to be networked)
///
/// We only add the physical properties on the ball that is displayed on screen (i.e the Interpolated ball)
/// We want the ball to be rigid so that when players collide with it, they bounce off.
///
/// However we remove the Position because we want the balls position to be interpolated, without being computed/updated
/// by the physics engine? Actually this shouldn't matter because we run interpolation in PostUpdate...
fn add_ball_physics(
    mut commands: Commands,
    mut ball_query: Query<
        Entity,
        (
            With<BallMarker>,
            Or<(Added<Interpolated>, Added<Predicted>)>,
        ),
    >,
) {
    for entity in ball_query.iter_mut() {
        commands
            .entity(entity)
            .insert((PhysicsBundle::ball(), GameCleanUp));
    }
}

/// When we receive other players (whether they are predicted or interpolated), we want to add the physics components
/// so that our predicted entities can predict collisions with them correctly
fn add_player_physics(
    connection: Res<ClientConnection>,
    mut commands: Commands,
    mut player_query: Query<
        (Entity, &PlayerId),
        (
            // insert the physics components on the player that is displayed on screen
            // (either interpolated or predicted)
            Or<(Added<Interpolated>, Added<Predicted>)>,
        ),
    >,
) {
    let client_id = connection.id();
    for (entity, player_id) in player_query.iter_mut() {
        if player_id.0 == client_id {
            // only need to do this for other players' entities
            debug!(
                ?entity,
                ?player_id,
                "we only want to add physics to other player! Skip."
            );
            continue;
        }
        info!(?entity, ?player_id, "adding physics to predicted player");
        commands
            .entity(entity)
            .insert((PhysicsBundle::player(), GameCleanUp));
    }
}

// The client input only gets applied to predicted entities that we own
// This works because we only predict the user's controlled entity.
// If we were predicting more entities, we would have to only apply movement to the player owned one.
fn player_movement(
    tick_manager: Res<TickManager>,
    mut velocity_query: Query<
        (
            Entity,
            &PlayerId,
            &Position,
            &mut LinearVelocity,
            &ActionState<PlayerActions>,
        ),
        With<Predicted>,
    >,
) {
    for (entity, _player_id, position, velocity, action_state) in velocity_query.iter_mut() {
        trace!(?entity, tick = ?tick_manager.tick(), ?position, actions = ?action_state.get_pressed(), "applying movement to predicted player");
        // note that we also apply the input to the other predicted clients! even though
        //  their inputs are only replicated with a delay!
        // TODO: add input decay?
        shared_movement_behaviour(velocity, action_state);
    }
}

// When the predicted copy of the client-owned entity is spawned, do stuff
// - assign it a different saturation
// - keep track of it in the Global resource
pub(crate) fn handle_predicted_spawn(mut predicted: Query<&mut ColorComponent, Added<Predicted>>) {
    for mut color in predicted.iter_mut() {
        let hsva = Hsva {
            saturation: 0.4,
            ..Hsva::from(color.0)
        };
        color.0 = Color::from(hsva);
    }
}

// When the interpolated copy of the client-owned entity is spawned, do stuff
// - assign it a different color
pub(crate) fn handle_interpolated_spawn(
    mut interpolated: Query<&mut ColorComponent, Added<Interpolated>>,
) {
    for mut color in interpolated.iter_mut() {
        let hsva = Hsva {
            saturation: 0.1,
            ..Hsva::from(color.0)
        };
        color.0 = Color::from(hsva);
    }
}
