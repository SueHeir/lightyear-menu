use bevy::prelude::*;
// use iyes_perf_ui::prelude::{PerfUiEntryFPS, PerfUiRoot, PerfUiWidgetBar};
use lightyear::{prelude::client::Predicted, shared::replication::components::Controlled};

use crate::{networking::protocol::PlayerId, GameState};

#[derive(Component)]
pub struct OuterCamera;

pub(crate) struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera).add_systems(
            Update,
            camera_follow_player.run_if(in_state(GameState::Game)),
        );
    }
}

fn setup_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // the "outer" camera renders whatever is on `HIGH_RES_LAYERS` to the screen.
    // here, the canvas and one of the sample sprites will be rendered by this camera
    commands.spawn((Camera2d, Msaa::Off, OuterCamera)); //.with_child((PixelCanvas, Sprite::from_image(image_handle), Canvas, HIGH_RES_LAYERS));

    // commands.spawn(PerfUiAllEntries::default());
    // commands.spawn((
    //     PerfUiRoot::default(),
    //     PerfUiWidgetBar::new(PerfUiEntryFPS::default()),
    //     // ...
    //  ));
}

fn camera_follow_player(
    // local_players: Res<LocalPlayers>,
    players: Query<(&PlayerId, &Transform, Has<Controlled>), With<Predicted>>,
    mut cameras: Query<
        (&mut Transform, &OrthographicProjection),
        (With<OuterCamera>, Without<PlayerId>),
    >,
) {
    for (_player, player_transform, controlled) in players.iter() {
        if controlled {
            let pos = player_transform.translation;

            for (mut transform, projection) in &mut cameras {
                transform.translation.x = pos.x;
                transform.translation.y = pos.y;
            }
        }
    }
}
