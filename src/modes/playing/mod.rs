mod blocks;

use self::blocks::{Block, BlockKind, Connector, FallingBlock};
use crate::{Globals, Transition, HEIGHT, WIDTH};

use cogs_gamedev::{directions::Direction4, int_coords::ICoord};
use itertools::Itertools;

use std::{collections::HashMap, f32::consts::TAU};

// In block coordinates, (0, 0) is the middle of the very top of the chasm.
// Y increases down. 0 is the level where the ground begins (so it's inside the ground.)

const CHASM_WIDTH: isize = 9;
/// How many grid squares across the whole screen would be
const SCREEN_WIDTH: isize = (WIDTH / BLOCK_SIZE) as isize;
/// How many grid squares down the whole screen would be
const SCREEN_HEIGHT: isize = (HEIGHT / BLOCK_SIZE) as isize;
/// The number of tiles you can look after the last tile
const BOTTOM_VIEW_SIZE: isize = SCREEN_HEIGHT + 1;

const FALL_VELOCITY: f32 = 2.0 / 60.0;

const BLOCK_SIZE: f32 = 16.0;

const SCROLL_HOTZONE_SIZE: f32 = 1.0 / 12.0;
const SCROLL_SPEED: f32 = 0.45;

#[derive(Clone)]
pub struct ModePlaying {
    /// Maps coordinates to whatever block is there.
    stable_blocks: HashMap<ICoord, Block>,
    /// Blocks visually falling right now
    falling_blocks: Vec<FallingBlock>,

    /// How far down I have scrolled.
    /// When this is 0, block (0, 0) is in the dead center of the screen
    scroll_depth: f32,

    /// Cached maximum depth value
    max_depth: isize,
    /// Cached center of mass
    center_of_mass: f32,
}

impl ModePlaying {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut stable_blocks = HashMap::new();
        // Embed blocks into the ground facing inwards.
        for side in 0..2 {
            for depth in 0..4 {
                let x = (CHASM_WIDTH + 1) / 2 * if side == 0 { -1 } else { 1 };
                let y = depth;

                let conn = Connector::sample();
                let mut connectors = [None, None, None, None];
                let dir = if side == 0 {
                    Direction4::East
                } else {
                    Direction4::West
                };
                connectors[dir as usize] = Some(conn);

                stable_blocks.insert(
                    ICoord::new(x, y),
                    Block {
                        connectors,
                        kind: BlockKind::Anchor,
                    },
                );
            }
        }

        Self {
            stable_blocks,
            falling_blocks: Vec::new(),
            scroll_depth: 0.0,
            max_depth: 0,
            center_of_mass: 0.0,
        }
    }

    pub fn update(&mut self, globals: &mut Globals) -> Transition {
        self.handle_input(globals);

        // Check for blocks that should fall
        let mut max_depth = 0;
        let mut superposes = 0.0;
        let mut masses = 0.0;
        let keys_to_remove = &self
            .stable_blocks
            .iter()
            .filter_map(|(pos, block)| {
                let keep = Self::is_stable(&self.stable_blocks, *pos, block);
                if keep {
                    max_depth = max_depth.max(pos.y);
                    superposes += pos.y as f32 * block.mass();
                    masses += block.mass();
                    None
                } else {
                    Some(*pos)
                }
            })
            .collect_vec();
        self.max_depth = max_depth;
        self.center_of_mass = if masses == 0.0 {
            // imagine having division by zero errors couldn't be me
            0.0
        } else {
            superposes / masses
        };

        for key in keys_to_remove {
            if let Some(block) = self.stable_blocks.remove(&key) {
                self.falling_blocks.push(FallingBlock {
                    block,
                    x: key.x,
                    y: key.y as f32,
                });
            }
            // else something funky happened...
        }

        // Update falling blocks
        // do this stupid backwards dance because of borrow errors
        for idx in (0..self.falling_blocks.len()).rev() {
            let faller = self.falling_blocks.get_mut(idx).unwrap();
            faller.y += FALL_VELOCITY;

            if faller.y > (self.max_depth + BOTTOM_VIEW_SIZE * 2) as f32 {
                self.falling_blocks.remove(idx);
                continue;
            }

            let rounded_pos = ICoord::new(faller.x, faller.y.floor() as isize);
            let links = Self::is_stable(&self.stable_blocks, rounded_pos, &faller.block);
            if links && !self.stable_blocks.contains_key(&rounded_pos) {
                // put it back in the map
                let faller = self.falling_blocks.remove(idx);
                self.stable_blocks.insert(rounded_pos, faller.block);
            }
        }

        Transition::None
    }

    fn handle_input(&mut self, globals: &mut Globals) {
        use macroquad::prelude::*;

        let (mx, my) = mouse_position();
        let scroll_y = mouse_wheel().1;
        let hotzone_size = screen_height() * SCROLL_HOTZONE_SIZE;
        if my < hotzone_size {
            self.scroll_depth -= SCROLL_SPEED * (hotzone_size - my) / hotzone_size;
        }
        if scroll_y > 0.0 {
            // mouse wheel seems to only trigger every few frames so we speed it up;
            self.scroll_depth -= 2.0 * SCROLL_SPEED;
        }
        if my > screen_height() - hotzone_size {
            self.scroll_depth +=
                SCROLL_SPEED * (my - screen_height() + hotzone_size) / hotzone_size;
        }
        if scroll_y < 0.0 {
            self.scroll_depth += 2.0 * SCROLL_SPEED;
        }
        self.scroll_depth = self
            .scroll_depth
            .clamp(0.0, (self.max_depth + BOTTOM_VIEW_SIZE) as f32);
    }

    pub fn draw(&self, globals: &Globals) {
        use macroquad::prelude::*;

        clear_background(BLUE);

        // Draw background
        let top_row = self.scroll_depth.floor() as isize - SCREEN_HEIGHT / 2;
        for y_idx in -1..SCREEN_HEIGHT + 1 {
            let row = top_row + y_idx;
            if row < 0 {
                continue;
            }
            // i don't know why this 0.5 is needed
            let deficit = self.scroll_depth.fract() - 0.5;

            for x_idx in -1..SCREEN_WIDTH + 1 {
                let col = x_idx - SCREEN_WIDTH / 2;

                let (tex, rot) = if col.abs() < CHASM_WIDTH / 2 + 1 {
                    // we're inside the chasm
                    (globals.assets.textures.dark_dirt, 0.0)
                } else if row == 0 {
                    // we're at the top of the chasm
                    (globals.assets.textures.dirt_edge, -TAU / 4.0)
                } else if col.abs() == CHASM_WIDTH / 2 + 1 {
                    // we're at the chasm edge
                    let rot = if col > 0 { TAU / 2.0 } else { 0.0 };
                    (globals.assets.textures.dirt_edge, rot)
                } else {
                    // we're in the chasm body
                    let rot = if col > 0 { TAU / 2.0 } else { 0.0 };
                    (globals.assets.textures.dirt_body, rot)
                };

                let center_x = x_idx as f32 * BLOCK_SIZE;
                let center_y = (y_idx as f32 - deficit) * BLOCK_SIZE;
                draw_texture_ex(
                    tex,
                    center_x - BLOCK_SIZE / 2.0,
                    center_y - BLOCK_SIZE / 2.0,
                    WHITE,
                    DrawTextureParams {
                        rotation: rot,
                        ..Default::default()
                    },
                );
            }
        }

        for (&pos, block) in self.stable_blocks.iter() {
            let (cx, cy) = self.block_to_pixel(pos);
            // TODO: don't draw blocks offscreen?
            block.draw_absolute(cx, cy, globals);
        }

        draw_text(
            format!("COM: {}; depth: {}", self.center_of_mass, self.max_depth).as_str(),
            16.0,
            16.0,
            20.0,
            WHITE,
        );
    }

    /// Check if a connector here facing in the specified direction would connect
    fn would_link(
        stable_blocks: &HashMap<ICoord, Block>,
        position: ICoord,
        connector: &Connector,
        facing: Direction4,
    ) -> bool {
        let target = position + facing.deltas();
        if let Some(block) = stable_blocks.get(&target) {
            let flip_dir = facing.flip();
            match &block.connectors[flip_dir as usize] {
                // ok this block has something; does it match?
                Some(conn) => conn.links_with(connector),
                // nothing matches with a smooth face
                None => false,
            }
        } else {
            // can't match with empty air
            false
        }
    }

    /// Check if this block can remain stable here: either it links up or rests on a block.
    fn is_stable(stable_blocks: &HashMap<ICoord, Block>, pos: ICoord, block: &Block) -> bool {
        block.kind == BlockKind::Anchor
            || stable_blocks.get(&(pos + ICoord::new(0, 1))).is_some()
            || Direction4::DIRECTIONS.iter().any(|&dir| {
                if let Some(conn) = &block.connectors[dir as usize] {
                    // It sticks if links to there
                    Self::would_link(stable_blocks, pos, conn, dir)
                } else {
                    false
                }
            })
    }

    fn block_to_pixel(&self, pos: ICoord) -> (f32, f32) {
        let cx = pos.x as f32 * BLOCK_SIZE + WIDTH / 2.0;
        let cy = (pos.y as f32 - self.scroll_depth) * BLOCK_SIZE + HEIGHT / 2.0;
        (cx, cy)
    }
}
