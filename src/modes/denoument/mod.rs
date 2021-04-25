use crate::{
    drawutils::{self, mouse_position_pixel},
    Gamemode, Globals, Transition,
};

use macroquad::prelude::*;

use super::{ModePlaying, ModeTitle};

#[derive(Clone)]
pub struct ModeDenoument {
    score: f32,
}

impl ModeDenoument {
    pub fn new(score: f32) -> Self {
        Self { score }
    }

    pub fn update(&mut self, globals: &mut Globals) -> Transition {
        let mouse = mouse_position_pixel().into();
        if is_mouse_button_pressed(MouseButton::Left) {
            if Rect::new(77.0, 137.0, 123.0, 19.0).contains(mouse) {
                Transition::Swap(Gamemode::Playing(ModePlaying::new()))
            } else if Rect::new(77.0, 161.0, 51.0, 19.0).contains(mouse) {
                Transition::Swap(Gamemode::Title(ModeTitle::new()))
            } else {
                Transition::None
            }
        } else {
            Transition::None
        }
    }

    pub fn draw(&self, globals: &Globals) {
        clear_background(WHITE);
        draw_texture(globals.assets.textures.denoument, 0.0, 0.0, WHITE);
        drawutils::draw_number(self.score.round() as i32, 177.0, 92.0, globals);
    }
}
