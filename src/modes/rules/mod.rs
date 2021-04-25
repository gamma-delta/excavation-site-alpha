use crate::{Globals, Transition};

use macroquad::prelude::*;

#[derive(Clone)]
pub struct ModeRules {}

impl ModeRules {
    pub fn new() -> Self {
        Self {}
    }

    pub fn update(&mut self, globals: &mut Globals) -> Transition {
        if is_mouse_button_pressed(MouseButton::Left) {
            Transition::Pop
        } else {
            Transition::None
        }
    }

    pub fn draw(&self, globals: &Globals) {
        clear_background(WHITE);
        draw_texture(globals.assets.textures.tutorial, 0.0, 0.0, WHITE);
    }
}
