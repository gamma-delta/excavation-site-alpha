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

const FALL_ACCELLERATION: f32 = 1.0 / 60.0;
const FALL_TERMINAL: f32 = 0.5;

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
const BREAK_TIMER: u64 = 60;

#[derive(Clone)]
pub struct ModePlaying {
    /// Maps coordinates to whatever block is there.
    stable_blocks: HashMap<ICoord, Block>,
    /// Blocks visually falling right now.
    /// Each entry is a clump of together-falling blocks.
    falling_blocks: Vec<Vec<FallingBlock>>,
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

    audio: AudioSignals,

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
            audio: AudioSignals::default(),
            frames_elapsed: 0,
        }
    }

    pub fn update(&mut self, globals: &mut Globals) -> Transition {
        self.audio = AudioSignals::default();
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

        for (pos, mut chance) in poses_to_break_chance {
            if !depths_with_rows.contains(&pos.y) {
                chance *= 0.1;
            }
            let entry = self.stable_blocks.entry(pos);
            if let Entry::Occupied(mut occupied) = entry {
                let block = occupied.get_mut();
                if self.frames_elapsed % BREAK_TIMER == 0 && QuadRand.gen_bool(chance) {
                    block.damage += 1;
                    self.audio.damage = true;
                }
                if block.damage > block.resilience() {
                    // die
                    occupied.remove_entry();
                }
            } // else we got a problem}
        }

        // Check for blocks that should fall
        let mut queries = self
            .stable_blocks
            .iter()
            .filter_map(|(pos, block)| {
                if block.kind == BlockKind::Anchor {
                    Some(*pos)
                } else {
                    None
                }
            })
            .collect_vec();
        let mut stable_poses = HashSet::new();
        while let Some(pos) = queries.pop() {
            if stable_poses.insert(pos) {
                // i've never met this coord in my life
                if let Some(block) = self.stable_blocks.get(&pos) {
                    queries.push(pos + ICoord::new(0, -1));
                    for &dir in &[Direction4::South, Direction4::East, Direction4::West] {
                        let neighbor_pos = pos + dir.deltas();
                        if let Some(neighbor) = self.stable_blocks.get(&neighbor_pos) {
                            let connects = match (
                                &block.connectors[dir as usize],
                                &neighbor.connectors[dir.flip() as usize],
                            ) {
                                (Some(a), Some(b)) => a.links_with(b),
                                _ => false,
                            };
                            if connects {
                                queries.push(neighbor_pos);
                            }
                        }
                    }
                }
            }
        }

        let falling_chunk = self
            .stable_blocks
            .drain_filter(|pos, _| !stable_poses.contains(pos))
            .collect_vec();
        self.audio.fall = !falling_chunk.is_empty();

        let falling_chunk = falling_chunk
            .into_iter()
            .map(|(pos, block)| {
                // if we do it at least once set the fall
                self.audio.fall = true;
                FallingBlock {
                    block,
                    x: pos.x,
                    y: pos.y as f32,
                    time_alive: 0,
                }
            })
            .collect_vec();
        println!("{:?}", &falling_chunk);
        self.falling_blocks.push(falling_chunk);

        // Update falling blocks
        // do this stupid backwards dance because of borrow errors
        for chunk_idx in (0..self.falling_blocks.len()).rev() {
            let chunk = self.falling_blocks.get_mut(chunk_idx).unwrap();
            let mut remove_this = None;
            'block: for faller_idx in (0..chunk.len()).rev() {
                let faller = chunk.get_mut(faller_idx).unwrap();
                let original_y = faller.y;
                faller.y += (FALL_ACCELLERATION * faller.time_alive as f32).min(FALL_TERMINAL);
                let delta = faller.y as isize - (original_y as isize - 1);
                for diff in 0..delta {
                    let passed_y = faller.y as isize - diff;
                    if passed_y > (self.max_depth + BOTTOM_VIEW_SIZE * 2) {
                        chunk.remove(faller_idx);
                        continue 'block;
                    }

                    let rounded_pos = ICoord::new(faller.x, passed_y);
                    let links = Self::is_stable(&self.stable_blocks, rounded_pos, &faller.block);
                    if links {
                        remove_this = Some(diff);
                        break 'block;
                    }
                }

                faller.time_alive += 1;
            }

            if let Some(diff) = remove_this {
                // noice
                let chunk = self.falling_blocks.remove(chunk_idx);
                'block2: for faller in chunk {
                    for cheat_up in 0..20 {
                        let pos = ICoord::new(faller.x, faller.y as isize - diff - cheat_up);
                        if !self.stable_blocks.contains_key(&pos) {
                            self.stable_blocks.insert(pos, faller.block);
                            continue 'block2;
                        }
                    }
                }
            }
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
                        self.audio.pick_up = true;
                    }
                }

                if is_mouse_button_pressed(MouseButton::Left) {
                    let blockpos = self.pixel_to_block(mx, my);
                    match self.stable_blocks.get_mut(&blockpos) {
                        Some(block) if block.is_removable() => {
                            block.damage += 1;
                            self.audio.damage = true;
                        }
                        _ => {}
                    }
                }
            }
            Some(info) => {
                if scroll_y > 0.0 {
                    self.conveyor_blocks[info.idx].connectors.rotate_left(1);
                    self.audio.rotate = true;
                } else if scroll_y < 0.0 {
                    self.conveyor_blocks[info.idx].connectors.rotate_right(1);
                    self.audio.rotate = true;
                }

                if !is_mouse_button_down(MouseButton::Left) {
                    let idx = info.idx;
                    let blockpos = self.pixel_to_block(mx, my);

                    let block = self.conveyor_blocks.get(idx).unwrap();
                    let valid_pos = block.is_valid_pos(blockpos);
                    let anchored_ok = if block.kind == BlockKind::Anchor {
                        // anchors must match up in order to be placed
                        Self::can_anchor_be_placed(&self.stable_blocks, blockpos, block)
                    } else {
                        true
                    };

                    if valid_pos && anchored_ok && !self.stable_blocks.contains_key(&blockpos) {
                        // poggers
                        let block = self.conveyor_blocks.remove(idx);
                        self.stable_blocks.insert(blockpos, block);
                        self.conveyor_blocks.push(QuadRand.gen());
                        self.audio.put_down = true;
                    } else {
                        self.audio.rotate = true;
                    }
                    // in any case stop holding it
                    self.held = None;
                }
            }
        }
    }

    pub fn draw(&self, globals: &Globals) {
        use macroquad::{audio::*, prelude::*};

        if self.frames_elapsed == 0 {
            play_sound(
                globals.assets.sounds.engineer_gaming,
                PlaySoundParams {
                    looped: true,
                    volume: 0.7,
                },
            );
        }
        let mut sounds = vec![];
        if self.audio.damage {
            sounds.push(globals.assets.sounds.damage);
        }
        if self.audio.fall {
            sounds.push(globals.assets.sounds.fall);
        }
        if self.audio.pick_up {
            sounds.push(globals.assets.sounds.pickup);
        }
        if self.audio.put_down {
            sounds.push(globals.assets.sounds.putdown);
        }
        if self.audio.rotate {
            sounds.push(globals.assets.sounds.rotate);
        }
        for sound in sounds {
            play_sound(
                sound,
                PlaySoundParams {
                    looped: false,
                    volume: 1.0,
                },
            );
        }

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
        for chunk in self.falling_blocks.iter() {
            for block in chunk.iter() {
                let fake_coord = ICoord::new(block.x, 0);
                let (cx, _) = self.block_to_pixel(fake_coord);
                let cy = (block.y - self.scroll_depth) * BLOCK_SIZE + HEIGHT / 2.0;
                block.block.draw_absolute(cx, cy, globals);
            }
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
                    Self::can_anchor_be_placed(&self.stable_blocks, blockpos, block)
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

    fn can_anchor_be_placed(
        stable_blocks: &HashMap<ICoord, Block>,
        pos: ICoord,
        block: &Block,
    ) -> bool {
        stable_blocks.contains_key(&(pos + ICoord::new(0, -1)))
            || Self::is_stable_anchorless(stable_blocks, pos, block)
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

#[derive(Clone, Default)]
struct AudioSignals {
    pick_up: bool,
    rotate: bool,
    fall: bool,
    put_down: bool,
    damage: bool,
}
