mod blocks;

use self::blocks::{Block, BlockKind, Connector, FallingBlock};
use crate::{drawutils, Globals, Transition, HEIGHT, WIDTH};

use cogs_gamedev::{directions::Direction4, int_coords::ICoord};
use drawutils::mouse_position_pixel;
use itertools::Itertools;
use quad_rand::compat::QuadRand;
use rand::Rng;

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    f32::consts::TAU,
};

// In block coordinates, (0, 0) is the middle of the very top of the chasm.
// Y increases down. 0 is the level where the ground begins (so it's inside the ground.)

const CHASM_WIDTH: isize = 9;
/// How many grid squares across the whole screen would be
const SCREEN_WIDTH: isize = (WIDTH / BLOCK_SIZE) as isize;
/// How many grid squares down the whole screen would be
const SCREEN_HEIGHT: isize = (HEIGHT / BLOCK_SIZE) as isize;
/// The number of tiles you can look after the last tile
const BOTTOM_VIEW_SIZE: isize = SCREEN_HEIGHT / 2;

const FALL_ACCELLERATION: f32 = 1.0 / 60.0 / 60.0;
// If this were more than 1 the block could skip through others
const FALL_TERMINAL: f32 = 0.9;

const BLOCK_SIZE: f32 = 16.0;

const SCROLL_HOTZONE_SIZE: f32 = 16.0;
const SCROLL_SPEED: f32 = 0.45;

const CONVEYOR_MAX_SIZE: usize = 7;
const CONVEYOR_Y_BOTTOM: f32 = 184.0;

/// Chance a block takes damage per frame based on the number of things it links to
const BREAK_CHANCES: [f64; 5] = [
    0.0, // a block resting never takes damage
    0.3 / 60.0,
    1.0 / 60.0,
    1.5 / 60.0,
    3.0 / 60.0,
];
const BREAK_TIMER: u64 = 30;

#[derive(Clone)]
pub struct ModePlaying {
    /// Maps coordinates to whatever block is there.
    stable_blocks: HashMap<ICoord, Block>,
    /// Blocks visually falling right now
    falling_blocks: Vec<FallingBlock>,
    /// Blocks in the conveyor on the side
    conveyor_blocks: Vec<Block>,
    /// Index in the conveyor of the block being held by the player right now
    held: Option<HoldInfo>,

    /// How far down I have scrolled.
    /// When this is 0, block (0, 0) is in the dead center of the screen
    scroll_depth: f32,

    /// Cached maximum depth value
    max_depth: isize,
    /// Cached center of mass
    center_of_mass: f32,

    frames_elapsed: u64,
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

                let conn = QuadRand.gen();
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
                        damage: 0,
                    },
                );
            }
        }

        let conveyor_blocks = (0..CONVEYOR_MAX_SIZE).map(|_| QuadRand.gen()).collect_vec();

        Self {
            stable_blocks,
            falling_blocks: Vec::new(),
            conveyor_blocks,
            held: None,
            scroll_depth: 0.0,
            max_depth: 0,
            center_of_mass: 0.0,
            frames_elapsed: 0,
        }
    }

    pub fn update(&mut self, globals: &mut Globals) -> Transition {
        self.handle_input(globals);

        // Damage blocks and record stats
        // Stability algorithm:
        // - Anchors have a stability of 1.
        // - The stability of any other block is
        let mut max_depth = 0;
        let mut superposes = 0.0;
        let mut masses = 0.0;
        let mut present_depths = HashSet::new();
        let poses_to_break_chance = self
            .stable_blocks
            .iter()
            .map(|(pos, block)| {
                max_depth = max_depth.max(pos.y);
                superposes += pos.y as f32 * block.mass();
                masses += block.mass();

                let link_count = Direction4::DIRECTIONS
                    .iter()
                    .filter(|dir| {
                        if let Some(conn) = &block.connectors[**dir as usize] {
                            Self::would_link(&self.stable_blocks, *pos, conn, **dir)
                        } else {
                            false
                        }
                    })
                    .count();
                let mut break_chance = BREAK_CHANCES[link_count];
                // Blocks by the wall are more bolstered
                if pos.x.abs() > CHASM_WIDTH / 2 {
                    break_chance /= 2.0;
                }
                present_depths.insert(pos.y);
                (*pos, break_chance)
            })
            .collect_vec();
        self.max_depth = max_depth;
        self.center_of_mass = if masses == 0.0 {
            // imagine having division by zero errors couldn't be me
            0.0
        } else {
            superposes / masses
        };

        let depths_with_rows = present_depths
            .into_iter()
            .filter(|depth| {
                // Check if all xposes have solid blocks
                (0..CHASM_WIDTH).all(|idx| {
                    let col = idx - CHASM_WIDTH / 2;
                    self.stable_blocks.contains_key(&ICoord::new(col, *depth))
                })
            })
            .collect_vec();

        if self.frames_elapsed % BREAK_TIMER == 0 {
            for (pos, chance) in poses_to_break_chance {
                if !depths_with_rows.contains(&pos.y) {
                    let entry = self.stable_blocks.entry(pos);
                    if let Entry::Occupied(mut occupied) = entry {
                        let block = occupied.get_mut();
                        if QuadRand.gen_bool(chance) {
                            block.damage += 1;
                            if block.damage > block.resilience() {
                                // die
                                occupied.remove_entry();
                            }
                        }
                    } // else we got a problem}
                }
            }
        }
        // Check for blocks that should fall
        // use a "union find"

        // Map nodes to their parents
        let mut parents = HashMap::<ICoord, ICoord>::new();
        let find_root = |pos: ICoord, parents: &HashMap<_, _>| {
            let mut current = pos;
            loop {
                let parent = parents.get(&current);
                if let Some(parent) = parent {
                    current = *parent;
                } else {
                    return current;
                }
            }
        };
        let unite = |a: ICoord, b: ICoord, parents: &mut HashMap<_, _>| {
            let root_a = find_root(a, parents);
            let root_b = find_root(b, parents);
            if root_a != root_b {
                // Always make an anchor the parent if i can
                let block_a = self.stable_blocks.get(&root_a);
                let block_b = self.stable_blocks.get(&root_b);

                let (kid, parent) = if matches!(block_a, Some(block) if block.kind == BlockKind::Anchor)
                {
                    (root_a, root_b)
                } else if matches!(block_b, Some(block) if block.kind == BlockKind::Anchor) {
                    (root_b, root_a)
                } else if QuadRand.gen_bool(0.5) {
                    // apparently it's best to flip a coin to pick
                    (root_a, root_b)
                } else {
                    (root_b, root_a)
                };
                parents.insert(kid, parent);
            }
        };

        for (&pos, block) in self.stable_blocks.iter() {
            // Only need to check left and down for matching
            for dir in &[Direction4::East, Direction4::South] {
                let neighbor_pos = pos + dir.deltas();

                let links = if let Some(neighbor) = self.stable_blocks.get(&neighbor_pos) {
                    matches!((
                    &block.connectors[*dir as usize],
                    &neighbor.connectors[dir.flip() as usize],
                ), (Some(a), Some(b)) if a.links_with(b))
                } else {
                    false
                };
                if links {
                    unite(pos, neighbor_pos, &mut parents);
                }
            }
        }
        // Now, for each block, check if it has the same root as an anchor block
        let anchor_roots = self
            .stable_blocks
            .iter()
            .filter_map(|(pos, block)| {
                if block.kind == BlockKind::Anchor {
                    Some(find_root(*pos, &parents))
                } else {
                    None
                }
            })
            .collect_vec();
        let keys_to_remove = self
            .stable_blocks
            .iter()
            .filter_map(|(pos, block)| {
                let bottom_pos = *pos + ICoord::new(0, 1);
                let bottom_support = self.stable_blocks.contains_key(&bottom_pos);
                if bottom_support {
                    // if we have bottom block, don't fall
                    None
                } else {
                    let root = find_root(*pos, &parents);
                    if anchor_roots.contains(&root) {
                        // nice keep this one
                        None
                    } else {
                        // die
                        Some(*pos)
                    }
                }
            })
            .collect_vec();
        for key in keys_to_remove {
            if let Some(block) = self.stable_blocks.remove(&key) {
                self.falling_blocks.push(FallingBlock {
                    block,
                    x: key.x,
                    y: key.y as f32,
                    time_alive: 0,
                });
            }
            // else something funky happened...
        }

        // Update falling blocks
        // do this stupid backwards dance because of borrow errors
        for idx in (0..self.falling_blocks.len()).rev() {
            let faller = self.falling_blocks.get_mut(idx).unwrap();
            faller.y +=
                (0.5 * FALL_ACCELLERATION * faller.time_alive.pow(2) as f32).min(FALL_TERMINAL);

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
                continue;
            }
            faller.time_alive += 1;
        }

        self.frames_elapsed += 1;
        Transition::None
    }

    fn handle_input(&mut self, globals: &mut Globals) {
        use macroquad::prelude::*;

        let (mx, my) = mouse_position_pixel();

        let scroll_y = mouse_wheel().1;
        if my < SCROLL_HOTZONE_SIZE {
            self.scroll_depth -= SCROLL_SPEED * (SCROLL_HOTZONE_SIZE - my) / SCROLL_HOTZONE_SIZE;
        }
        if self.held.is_none() && scroll_y > 0.0 {
            // mouse wheel seems to only trigger every few frames so we speed it up;
            self.scroll_depth -= 2.0 * SCROLL_SPEED;
        }
        if my > HEIGHT - SCROLL_HOTZONE_SIZE {
            self.scroll_depth +=
                SCROLL_SPEED * (my - HEIGHT + SCROLL_HOTZONE_SIZE) / SCROLL_HOTZONE_SIZE;
        }
        if self.held.is_none() && scroll_y < 0.0 {
            self.scroll_depth += 2.0 * SCROLL_SPEED;
        }
        self.scroll_depth = self
            .scroll_depth
            .clamp(0.0, (self.max_depth + BOTTOM_VIEW_SIZE) as f32);

        match &mut self.held {
            None => {
                if is_mouse_button_down(MouseButton::Left)
                    && mx > WIDTH - 64.0
                    && mx < WIDTH - 32.0
                    && my > 40.0
                    && my < 200.0
                {
                    // we're in the conveyor pickup zone
                    let remainder = (CONVEYOR_Y_BOTTOM - my + BLOCK_SIZE) % 24.0;
                    if remainder < 16.0 {
                        let idx = ((CONVEYOR_Y_BOTTOM - my + BLOCK_SIZE) / 24.0) as usize;
                        self.held = Some(HoldInfo { idx });
                    }
                }
            }
            Some(info) => {
                if scroll_y > 0.0 {
                    self.conveyor_blocks[info.idx].connectors.rotate_left(1);
                } else if scroll_y < 0.0 {
                    self.conveyor_blocks[info.idx].connectors.rotate_right(1);
                }

                if !is_mouse_button_down(MouseButton::Left) {
                    let idx = info.idx;
                    let blockpos = self.pixel_to_block(mx, my);

                    let block = self.conveyor_blocks.get(idx).unwrap();
                    let valid_pos = block.is_valid_pos(blockpos);
                    let anchored_ok = if block.kind == BlockKind::Anchor {
                        // anchors must match up in order to be placed
                        Self::is_stable_anchorless(&self.stable_blocks, blockpos, block)
                    } else {
                        true
                    };

                    if valid_pos && anchored_ok && !self.stable_blocks.contains_key(&blockpos) {
                        // poggers
                        let block = self.conveyor_blocks.remove(idx);
                        self.stable_blocks.insert(blockpos, block);
                        self.conveyor_blocks.push(QuadRand.gen());
                    }
                    // in any case stop holding it
                    self.held = None;
                }
            }
        }
    }

    pub fn draw(&self, globals: &Globals) {
        use macroquad::prelude::*;

        let (mx, my) = mouse_position_pixel();

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
        for block in self.falling_blocks.iter() {
            let fake_coord = ICoord::new(block.x, 0);
            let (cx, _) = self.block_to_pixel(fake_coord);
            let cy = (block.y - self.scroll_depth) * BLOCK_SIZE + HEIGHT / 2.0;
            block.block.draw_absolute(cx, cy, globals);
        }

        // Draw the depth meter
        let pixel_depth =
            ((self.center_of_mass - self.scroll_depth) * BLOCK_SIZE + HEIGHT / 2.0).round();
        draw_line(
            BLOCK_SIZE * 2.0,
            pixel_depth,
            WIDTH + 10.0,
            pixel_depth,
            1.0,
            drawutils::hexcolor(0xffee83aa),
        );
        let corner_x = BLOCK_SIZE * 2.0 - 16.0;
        let corner_y = pixel_depth - 16.0;
        draw_texture(
            globals.assets.textures.depth_meter,
            corner_x,
            corner_y,
            WHITE,
        );

        // Draw the number
        let depth_string = format!("{:.0}", self.center_of_mass);
        for (idx, c) in depth_string.chars().rev().enumerate() {
            let cx = corner_x + 23.0 - (4 * idx) as f32;
            let cy = corner_y + 13.0;

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

        // Draw the conveyor
        draw_texture(globals.assets.textures.conveyor, WIDTH - 70.0, 0.0, WHITE);
        for (idx, block) in self.conveyor_blocks.iter().enumerate() {
            let (cx, cy, color) = if matches!(&self.held, Some(held) if held.idx == idx) {
                let blockpos = self.pixel_to_block(mx, my);
                let anchored_ok = if block.kind == BlockKind::Anchor {
                    // anchors must match up in order to be placed
                    Self::is_stable_anchorless(&self.stable_blocks, blockpos, block)
                } else {
                    true
                };
                if block.is_valid_pos(blockpos) && anchored_ok {
                    // we're at a good pos
                    let (cx, cy) = self.block_to_pixel(blockpos);
                    (cx, cy, Color::new(1.0, 1.0, 1.0, 0.8))
                } else {
                    (mx, my, Color::new(1.0, 1.0, 1.0, 0.7))
                }
            } else {
                let cx = WIDTH - 70.0 + 24.0 + BLOCK_SIZE / 2.0;
                let cy = CONVEYOR_Y_BOTTOM - idx as f32 * 24.0 + BLOCK_SIZE / 2.0;
                (cx, cy, WHITE)
            };

            block.draw_absolute_color(cx, cy, color, globals);
        }
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
        block.kind == BlockKind::Anchor || Self::is_stable_anchorless(stable_blocks, pos, block)
    }

    fn is_stable_anchorless(
        stable_blocks: &HashMap<ICoord, Block>,
        pos: ICoord,
        block: &Block,
    ) -> bool {
        stable_blocks.get(&(pos + ICoord::new(0, 1))).is_some()
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

    fn pixel_to_block(&self, x: f32, y: f32) -> ICoord {
        let block_x = (x / BLOCK_SIZE).round() as isize - SCREEN_WIDTH / 2;
        let block_y = (y / BLOCK_SIZE - 0.5).round() as isize - SCREEN_HEIGHT / 2
            + self.scroll_depth.round() as isize;
        ICoord::new(block_x, block_y)
    }
}

#[derive(Clone)]
struct HoldInfo {
    idx: usize,
}
