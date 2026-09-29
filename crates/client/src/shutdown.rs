use crate::{
    Movement,
    network::{Mode, Network},
};
use bevy::{app::AppExit, prelude::*};
#[derive(Resource, Default)]
pub struct Requested(pub bool);
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
        _ => {}
    }
}
