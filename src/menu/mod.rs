use std::{net::Ipv4Addr, str::FromStr};

use bevy::{app::AppExit, prelude::*};
use bevy_simple_text_input::{
    TextInput, TextInputSubmitEvent, TextInputSystem, TextInputTextColor, TextInputTextFont,
    TextInputValue,
};
use lightyear::prelude::{steamworks::FriendFlags, SteamId, SteamworksClient};
use steamworks::LobbyId;

// use crate::{networking::SteamworksResource, GameCleanUp, MultiplayerState};

use crate::{networking::client::ClientStartupResources, MultiplayerState};

use super::{despawn_screen, GameState, TEXT_COLOR};

// This plugin manages the menu, with 5 different screens:
// - a main menu with "New Game", "Settings", "Quit"
// - a settings menu with two submenus and a back button
// - two settings screen with a setting that can be set and a back button

pub(crate) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app
            // At start, the menu is not enabled. This will be changed in `menu_setup` when
            // entering the `GameState::Menu` state.
            // Current screen in the menu is handled by an independent state from `GameState`
            .init_state::<MenuState>()
            .add_systems(
                OnEnter(GameState::Menu),
                (menu_setup),
            )
            // Systems to handle the main menu screen
            .add_systems(OnEnter(MenuState::Main), main_menu_setup)
            .add_systems(OnEnter(MenuState::JoinServer), join_server_menu_setup)
            .add_systems(OnExit(MenuState::Main), despawn_screen::<OnMainMenuScreen>)
            // Systems to handle the settings menu screen
            .add_systems(
                OnExit(MenuState::JoinServer),
                despawn_screen::<OnJoinServerMenuScreen>,
            )
            // Common systems to all screens that handles buttons behavior
            .add_systems(
                Update,
                (menu_action, button_system).run_if(in_state(GameState::Menu)),
            )
            .add_systems(Update, listener.after(TextInputSystem));
        
        app.add_systems(Update, client_accepts_join_game.run_if(
            in_state(MultiplayerState::None).and(in_state(GameState::Menu)),
        ));
    }
}

// State used for the current menu screen
#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, States)]
enum MenuState {
    Main,
    JoinServer,
    #[default]
    Disabled,
}

// Tag component used to tag entities added on the main menu screen
#[derive(Component)]
struct OnMainMenuScreen;

// Tag component used to tag entities added on the settings menu screen
#[derive(Component)]
struct OnJoinServerMenuScreen;

const NORMAL_BUTTON: Color = Color::srgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.25, 0.25, 0.25);
const HOVERED_PRESSED_BUTTON: Color = Color::srgb(0.25, 0.65, 0.25);
const PRESSED_BUTTON: Color = Color::srgb(0.35, 0.75, 0.35);

const BORDER_COLOR_ACTIVE: Color = Color::srgb(0.75, 0.52, 0.99);
const BACKGROUND_COLOR: Color = Color::srgb(0.15, 0.15, 0.15);

// Tag component used to mark which setting is currently selected
#[derive(Component)]
struct SelectedOption;

// All actions that can be triggered from a button click
#[derive(Component)]
enum MenuButtonAction {
    SeperateAndJoin,
    JoinServerScreen,
    MainMenu,
    JoinSteamFriend((SteamId, LobbyId)),
    JoinServer,
    Quit,
}

// This system handles changing all buttons color based on mouse interaction
fn button_system(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, Option<&SelectedOption>),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, mut background_color, selected) in &mut interaction_query {
        *background_color = match (*interaction, selected) {
            (Interaction::Pressed, _) | (Interaction::None, Some(_)) => PRESSED_BUTTON.into(),
            (Interaction::Hovered, Some(_)) => HOVERED_PRESSED_BUTTON.into(),
            (Interaction::Hovered, None) => HOVERED_BUTTON.into(),
            (Interaction::None, None) => NORMAL_BUTTON.into(),
        }
    }
}

fn menu_setup(
    mut menu_state: ResMut<NextState<MenuState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
) {
    menu_state.set(MenuState::Main);
    multiplayer_state.set(MultiplayerState::None);
}

fn main_menu_setup(mut commands: Commands) {
    // Common style for all buttons on the screen
    let button_node = Node {
        width: Val::Px(300.0),
        height: Val::Px(65.0),
        margin: UiRect::all(Val::Px(20.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };

    let button_text_font = TextFont {
        font_size: 33.0,
        ..default()
    };

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            OnMainMenuScreen,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::Srgba(Srgba {
                        red: 36.0 / 255.0,
                        green: 22.0 / 255.0,
                        blue: 39.0 / 255.0,
                        alpha: 255.0 / 255.0,
                    })),
                ))
                .with_children(|parent| {
                    // Display the game name
                    parent.spawn((
                        Text::new("Menu Example"),
                        TextFont {
                            font_size: 67.0,
                            ..default()
                        },
                        TextColor(TEXT_COLOR),
                        Node {
                            margin: UiRect::all(Val::Px(50.0)),
                            ..default()
                        },
                    ));

                    parent
                        .spawn((
                            Button,
                            button_node.clone(),
                            BackgroundColor(NORMAL_BUTTON),
                            MenuButtonAction::SeperateAndJoin,
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new("Play"),
                                button_text_font.clone(),
                                TextColor(TEXT_COLOR),
                            ));
                        });


                    parent
                        .spawn((
                            Button,
                            button_node.clone(),
                            BackgroundColor(NORMAL_BUTTON),
                            MenuButtonAction::JoinServerScreen,
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new("Join Server"),
                                button_text_font.clone(),
                                TextColor(TEXT_COLOR),
                            ));
                        });
                        

                    parent
                        .spawn((
                            Button,
                            button_node,
                            BackgroundColor(NORMAL_BUTTON),
                            MenuButtonAction::Quit,
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new("Quit"),
                                button_text_font,
                                TextColor(TEXT_COLOR),
                            ));
                        });
                });
        });
}

fn menu_action(
    interaction_query: Query<
        (&Interaction, &MenuButtonAction),
        (Changed<Interaction>, With<Button>),
    >,
    mut app_exit_events: EventWriter<AppExit>,
    mut menu_state: ResMut<NextState<MenuState>>,
    mut game_state: ResMut<NextState<GameState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut client_setup_info: ResMut<crate::ClientConfigInfo>,
) {
    for (interaction, menu_button_action) in &interaction_query {
        if *interaction == Interaction::Pressed {
            match menu_button_action {
                MenuButtonAction::Quit => {
                    app_exit_events.write(AppExit::Success);
                }
                MenuButtonAction::JoinServerScreen => {
                    menu_state.set(MenuState::JoinServer);
                }
                MenuButtonAction::MainMenu => {
                    menu_state.set(MenuState::Main);
                }
                MenuButtonAction::JoinSteamFriend((id, lobby_id)) => {
                    client_setup_info.seperate_mode = false;
                    client_setup_info.steam_connect_to = Some((*id, *lobby_id));

                    game_state.set(GameState::Game);
                    menu_state.set(MenuState::Disabled);
                    multiplayer_state.set(MultiplayerState::Client)
                }
                MenuButtonAction::JoinServer => {
                    if Ipv4Addr::from_str(&client_setup_info.address).is_ok() {
                        // client_setup_info.address = text_input_value.single().0.clone();
                        client_setup_info.seperate_mode = false;
                        client_setup_info.steam_connect_to = None;
                        game_state.set(GameState::Game);
                        menu_state.set(MenuState::Disabled);
                        multiplayer_state.set(MultiplayerState::Client)
                    }
                }
                MenuButtonAction::SeperateAndJoin => {
                    client_setup_info.seperate_mode = true;
                    client_setup_info.steam_connect_to = None;
                    game_state.set(GameState::Game);
                    menu_state.set(MenuState::Disabled);
                    multiplayer_state.set(MultiplayerState::ClientSpawnServer);
                    // multiplayer_state.set(MultiplayerState::Client);
                },
            }
        }
    }
}


//Non menu actions that only happen in the menu

fn client_accepts_join_game(
    mut client_startup: ResMut<ClientStartupResources>,
    mut menu_state: ResMut<NextState<MenuState>>,
    mut game_state: ResMut<NextState<GameState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut client_setup_info: ResMut<crate::ClientConfigInfo>,) {

    if let Some(temp) = client_startup.steam_accept_join_game_request.clone() {
        if let Some(guard) = temp.try_lock() {
            if let Some(steam_id) = *guard {

                client_setup_info.seperate_mode = false;
                client_setup_info.steam_connect_to = Some((steam_id, LobbyId::from_raw(0)));

                game_state.set(GameState::Game);
                menu_state.set(MenuState::Disabled);
                multiplayer_state.set(MultiplayerState::Client)
            }
        }

        client_startup.steam_accept_join_game_request = None;
    }

}

fn join_server_menu_setup(mut commands: Commands, mut steamworks: Option<ResMut<SteamworksClient>>) {//mut steamworks: ResMut<SteamworksResource>
    let mut steam_friends = Vec::new();

    if let Some(steamworks) = steamworks.as_mut() {
         for friend in steamworks
        .0.friends().get_friends(FriendFlags::all()).iter()
        {
            if let Some(game_info) = friend.game_played() {
                if game_info.game.app_id().0 == 480 && game_info.lobby.raw() != 0 {
                    steam_friends.push((friend.name(), friend.id(), game_info.lobby));
                    println!(
                        "{:?} {:?} {:?} {:?} {:?} {:?}",
                        friend.name(),
                        friend.id(),
                        game_info.game_address,
                        game_info.game_port,
                        game_info.query_port,
                        game_info.lobby
                    )
                }
            }
        }
    } 
  

    // Common style for all buttons on the screen
    let button_node = Node {
        width: Val::Px(300.0),
        height: Val::Px(65.0),
        margin: UiRect::all(Val::Px(20.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };

    let button_text_font = TextFont {
        font_size: 33.0,
        ..default()
    };

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            OnJoinServerMenuScreen,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::Srgba(Srgba {
                        red: 36.0 / 255.0,
                        green: 22.0 / 255.0,
                        blue: 39.0 / 255.0,
                        alpha: 255.0 / 255.0,
                    })),
                ))
                .with_children(|parent| {
                    for (friend, id, lobby) in steam_friends {
                        parent
                            .spawn((
                                Button,
                                button_node.clone(),
                                BackgroundColor(NORMAL_BUTTON),
                                MenuButtonAction::JoinSteamFriend((id, lobby)),
                            ))
                            .with_children(|parent| {
                                parent.spawn((
                                    Text::new(friend),
                                    button_text_font.clone(),
                                    TextColor(TEXT_COLOR),
                                ));
                            });
                    }

                    parent.spawn((
                        Node {
                            width: Val::Px(300.0),
                            border: UiRect::all(Val::Px(5.0)),
                            padding: UiRect::all(Val::Px(5.0)),
                            ..default()
                        },
                        BorderColor(BORDER_COLOR_ACTIVE),
                        BackgroundColor(BACKGROUND_COLOR),
                        TextInput,
                        TextInputTextFont(TextFont {
                            font_size: 34.,
                            ..default()
                        }),
                        TextInputTextColor(TextColor(TEXT_COLOR)),
                        TextInputValue("127.0.0.1".to_string()),
                    ));

                    parent
                        .spawn((
                            Button,
                            button_node.clone(),
                            BackgroundColor(NORMAL_BUTTON),
                            MenuButtonAction::JoinServer,
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new("Connect"),
                                button_text_font.clone(),
                                TextColor(TEXT_COLOR),
                            ));
                        });

                    parent
                        .spawn((
                            Button,
                            button_node.clone(),
                            BackgroundColor(NORMAL_BUTTON),
                            MenuButtonAction::MainMenu,
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new("Back"),
                                button_text_font.clone(),
                                TextColor(TEXT_COLOR),
                            ));
                        });
                });
        });
}

fn listener(
    mut events: EventReader<TextInputSubmitEvent>,
    mut client_setup_info: ResMut<crate::ClientConfigInfo>,
    mut game_state: ResMut<NextState<GameState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut menu_state: ResMut<NextState<MenuState>>,
) {
    for event in events.read() {
        client_setup_info.address = event.value.clone();

        if Ipv4Addr::from_str(&client_setup_info.address).is_ok() {
            client_setup_info.seperate_mode = false;
            client_setup_info.steam_connect_to = None;
            game_state.set(GameState::Game);
            menu_state.set(MenuState::Disabled);
            multiplayer_state.set(MultiplayerState::Client)
        }
    }
}
