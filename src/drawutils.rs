use macroquad::prelude::*;

use crate::{wh_deficit, Globals, HEIGHT, WIDTH};

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

/// Draw a number.
/// `(cx, cy)` is the upper *right* corner of the number, growing to the left
pub fn draw_number(num: i32, corner_x: f32, corner_y: f32, globals: &Globals) {
    let depth_string = num.to_string();
    for (idx, c) in depth_string.chars().rev().enumerate() {
        let cx = corner_x - 3.0 - (4 * idx) as f32;
        let cy = corner_y;

        let sx = if let Some(digit) = c.to_digit(10) {
            digit
        } else if c == '-' {
            10
        } else {
            // hmm
            continue;
        };
        let sx = sx as f32 * 3.0;

        draw_texture_ex(
            globals.assets.textures.number_atlas,
            cx,
            cy,
            WHITE,
            DrawTextureParams {
                source: Some(Rect::new(sx, 0.0, 3.0, 5.0)),
                ..Default::default()
            },
        );
    }
}
