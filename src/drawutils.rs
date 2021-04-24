use macroquad::prelude::{
    mouse_position, mouse_position_local, screen_height, screen_width, Color,
};

use crate::{wh_deficit, HEIGHT, WIDTH};

/// Make a Color from an RRGGBBAA hex code.
pub fn hexcolor(code: u32) -> Color {
    let [r, g, b, a] = code.to_be_bytes();
    Color::from_rgba(r, g, b, a)
}

pub fn mouse_position_pixel() -> (f32, f32) {
    let (mx, my) = mouse_position();
    let (wd, hd) = wh_deficit();
    let mx = (mx - wd / 2.0) / ((screen_width() - wd) / WIDTH);
    let my = (my - hd / 2.0) / ((screen_height() - hd) / HEIGHT);
    (mx, my)
}
