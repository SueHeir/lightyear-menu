use bevy::{core_pipeline::{bloom::Bloom, tonemapping::Tonemapping}, prelude::*, window::WindowResized};

use crate::GameState;
// use iyes_perf_ui::prelude::{PerfUiEntryFPS, PerfUiRoot, PerfUiWidgetBar};


/// In-game resolution width.
pub const RES_WIDTH: u32 = 640;

/// In-game resolution height.
pub const RES_HEIGHT: u32 = 360;


#[derive(Component)]
pub struct OuterCamera;

pub(crate) struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera);
        // .add_systems(
        //     Update,
        //     camera_follow_player.run_if(in_state(GameState::Game)),
        // )
        // .add_systems(Update, fit_canvas);
    }
}

fn setup_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // the "outer" camera renders whatever is on `HIGH_RES_LAYERS` to the screen.
    // here, the canvas and one of the sample sprites will be rendered by this camera
    commands.spawn((Camera2d,
        Camera {
            hdr: true,
            
            ..default()
        },
        Tonemapping::TonyMcMapface,
        Bloom::default(),
        Visibility::default(),
        
        OuterCamera)); //.with_child((PixelCanvas, Sprite::from_image(image_handle), Canvas, HIGH_RES_LAYERS));

    // commands.spawn(PerfUiAllEntries::default());
    // commands.spawn((
    //     PerfUiRoot::default(),
    //     PerfUiWidgetBar::new(PerfUiEntryFPS::default()),
    //     // ...
    //  ));
}


// /// Scales camera projection to fit the window (integer multiples only).
// fn fit_canvas(
//     mut resize_events: EventReader<WindowResized>,
//     mut projection: Single<&mut OrthographicProjection, With<OuterCamera>>,
// ) {
//     for event in resize_events.read() {
//         let h_scale = event.width / RES_WIDTH as f32;
//         let v_scale = event.height / RES_HEIGHT as f32;
//         projection.scale = 0.3;
//     }
// }

// fn camera_follow_player(
//     // local_players: Res<LocalPlayers>,
//     players: Query<(&Player, &Transform, Has<Controlled>), With<Predicted>>,
//     mut cameras: Query<
//         (&mut Transform, &OrthographicProjection),
//         (With<OuterCamera>, Without<Player>),
//     >,
// ) {
//     for (_player, player_transform, controlled) in players.iter() {
//         if controlled {
//             let pos = player_transform.translation;

//             for (mut transform, projection) in &mut cameras {
//                 transform.translation.x = pos.x;
//                 transform.translation.y = pos.y;
//             }
//         }
//     }
// }
