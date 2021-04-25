use macroquad::{
    audio::play_sound_once,
    prelude::{clear_background, draw_texture, WHITE},
};

use crate::{
    drawutils::mouse_position_pixel, Gamemode, Globals, ModePlaying, ModeRules, Transition,
};

#[derive(Clone)]
pub struct ModeTitle {
    play_highlighted: bool,
    rules_highlighted: bool,

    play_click: bool,
}

impl ModeTitle {
    pub fn new() -> Self {
        Self {
            play_highlighted: false,
            rules_highlighted: false,
            play_click: false,
        }
    }

    pub fn update(&mut self, globals: &mut Globals) -> Transition {
        use macroquad::prelude::*;

        self.play_click = false;

        let (mx, my) = mouse_position_pixel();

        let play_rect = Rect::new(76.0, 121.0, 67.0, 23.0);
        let hovering_play = play_rect.contains(vec2(mx, my));
        if !self.play_highlighted && hovering_play {
            self.play_click = true;
        }
        self.play_highlighted = hovering_play;

        let rules_rect = Rect::new(76.0, 147.0, 83.0, 23.0);
        let hovering_rules = rules_rect.contains(vec2(mx, my));
        if !self.rules_highlighted && hovering_rules {
            self.play_click = true;
        }
        self.rules_highlighted = hovering_rules;

        if is_mouse_button_pressed(MouseButton::Left) {
            macroquad::rand::srand((mx.to_bits() as u64) + ((my.to_bits() as u64) << 32));
            if self.play_highlighted {
                Transition::Swap(Gamemode::Playing(ModePlaying::new()))
            } else if self.rules_highlighted {
                Transition::Push(Gamemode::Rules(ModeRules::new()))
            } else {
                Transition::None
            }
        } else {
            Transition::None
        }
    }

    pub fn draw(&self, globals: &Globals) {
        clear_background(WHITE);
        draw_texture(globals.assets.textures.title_screen, 0.0, 0.0, WHITE);

        if self.play_click {
            play_sound_once(globals.assets.sounds.rotate);
        }
    }
}
