//! Graceful exit from the menu's "Exit" entry: leave the world, wait until the server has
//! saved the hero (it answers with the character list), then close the app.
//! Closing the OS window bypasses this path.
use crate::{
    Movement,
    network::{Mode, Network},
};
use bevy::{app::AppExit, prelude::*};
/// Set by `menu::input`; stays true until the app exits.
#[derive(Resource, Default)]
pub struct Requested(pub bool);
/// Polled every frame, so it steps Online → Selection → exit over several frames.
pub fn update(
    requested: Res<Requested>,
    mut network: ResMut<Network>,
    mut movement: ResMut<Movement>,
    mut exit: MessageWriter<AppExit>,
) {
    if !requested.0 {
        return;
    }
    *movement = Movement::default();
    match network.mode {
        Mode::Online if !network.pending => network.leave_world(),
        // Characters arrives only after the server saves and removes the active actor.
        Mode::Selection if !network.pending => {
            exit.write(AppExit::Success);
        }
        Mode::Menu | Mode::Offline => {
            exit.write(AppExit::Success);
        }
        // Connecting or a request in flight: wait; the network timeout falls back to Menu.
        _ => {}
    }
}
