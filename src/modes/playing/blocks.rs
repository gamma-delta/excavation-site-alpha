use super::{BLOCK_SIZE, CHASM_WIDTH};
use crate::{assets::Textures, Globals};

use cogs_gamedev::{directions::Direction4, int_coords::ICoord};
use macroquad::prelude::{Color, Texture2D, WHITE};
use rand::{
    distributions::Standard,
    prelude::{Distribution, SliceRandom},
    Rng,
};

#[derive(Clone, Debug)]
pub struct Block {
    /// Maps `Direction4 as usize` to the connector
    pub connectors: [Option<Connector>; 4],
    pub kind: BlockKind,
    pub damage: u8,
}

impl Block {
    pub fn mass(&self) -> f32 {
        match self.kind {
            BlockKind::Scaffold => 1.0,
            BlockKind::Solid => 5.0,
            BlockKind::Anchor => 0.0,
        }
    }

    pub fn is_removable(&self) -> bool {
        match self.kind {
            BlockKind::Scaffold => true,
            BlockKind::Solid => false,
            BlockKind::Anchor => false,
        }
    }

    /// Return the amount of damage this can take
    pub fn resilience(&self) -> u8 {
        match self.kind {
            BlockKind::Scaffold => 8,
            BlockKind::Solid => 16,
            BlockKind::Anchor => 64,
        }
    }

    pub fn is_valid_pos(&self, pos: ICoord) -> bool {
        let valid_x = match self.kind {
            BlockKind::Anchor => pos.x.abs() == CHASM_WIDTH / 2 + 1,
            _ => pos.x.abs() < CHASM_WIDTH / 2 + 1,
        };
        let valid_y = pos.y >= 0;
        valid_x && valid_y
    }

    pub fn draw_absolute(&self, cx: f32, cy: f32, globals: &Globals) {
        self.draw_absolute_color(cx, cy, WHITE, globals);
    }

    pub fn draw_absolute_color(&self, cx: f32, cy: f32, color: Color, globals: &Globals) {
        use macroquad::prelude::*;

        let tex = self.kind.get_texture(&globals.assets.textures);
        let corner_x = cx - BLOCK_SIZE / 2.0;
        let corner_y = cy - BLOCK_SIZE / 2.0;
        draw_texture(tex, corner_x, corner_y, color);

        // Figure out how much damage to draw
        if self.damage > 0 {
            let damage_atlas = globals.assets.textures.damage_atlas;
            let max_damage = (damage_atlas.width() / damage_atlas.height()) as u8;
            // 0 = just a scratch; 1 = fully damaged
            let damage_scale = (self.damage - 1) as f32 / self.resilience() as f32;
            let damage_amt = (damage_scale * max_damage as f32).ceil();

            let sx = damage_amt * BLOCK_SIZE;
            draw_texture_ex(
                damage_atlas,
                corner_x,
                corner_y,
                color,
                DrawTextureParams {
                    source: Some(Rect::new(sx, 0.0, BLOCK_SIZE, BLOCK_SIZE)),
                    ..Default::default()
                },
            );
        }

        for (idx, conn) in self.connectors.iter().enumerate() {
            if let Some(conn) = conn {
                let dir = Direction4::DIRECTIONS[idx];

                let slice_x = conn.shape as usize * 2 + !conn.sticks_out as usize;
                let slice_x = slice_x as f32 * BLOCK_SIZE;

                let target_x = corner_x
                    + if !conn.sticks_out {
                        dir.deltas().x as f32 * BLOCK_SIZE
                    } else {
                        0.0
                    };
                let target_y = corner_y
                    + if !conn.sticks_out {
                        dir.deltas().y as f32 * BLOCK_SIZE
                    } else {
                        0.0
                    };

                // rotate about this center
                let cx = target_x + BLOCK_SIZE / 2.0;
                let cy = target_y + BLOCK_SIZE / 2.0;

                draw_texture_ex(
                    globals.assets.textures.connector_atlas,
                    target_x,
                    target_y,
                    color,
                    DrawTextureParams {
                        source: Some(Rect::new(slice_x, 0.0, BLOCK_SIZE, BLOCK_SIZE)),
                        rotation: if dir == Direction4::East {
                            0.0
                        } else {
                            dir.radians()
                        },
                        flip_y: dir == Direction4::East,
                        pivot: Some(vec2(cx, cy)),
                        ..Default::default()
                    },
                );
            }
        }
    }
}

impl Distribution<Block> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Block {
        if rng.gen_bool(0.05) {
            // small chance to make an anchor
            let mut connectors = [Some(rng.gen()), None, None, None];
            connectors.shuffle(rng);

            Block {
                connectors,
                kind: BlockKind::Anchor,
                damage: 0,
            }
        } else {
            let kind = rng.gen();
            // The connector must have at least one non-None value
            let mut connectors = [Some(rng.gen()), None, None, None];
            for item in connectors.iter_mut().skip(1) {
                *item = rng.gen();
            }
            connectors.shuffle(rng);

            Block {
                connectors,
                kind,
                damage: 0,
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FallingBlock {
    pub block: Block,
    pub x: isize,
    pub y: f32,
    pub time_alive: u64,
}

#[derive(Clone, Debug)]
pub struct Connector {
    pub shape: ConnectorShape,
    pub sticks_out: bool,
}

impl Connector {
    pub fn links_with(&self, other: &Connector) -> bool {
        self.shape == other.shape && self.sticks_out != other.sticks_out
    }
}

impl Distribution<Connector> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Connector {
        Connector {
            shape: rng.gen(),
            sticks_out: rng.gen(),
        }
    }
}

/// The shape of the connector on the side of the block
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ConnectorShape {
    Square,
    Round,
    Pointy,
}

impl Distribution<ConnectorShape> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> ConnectorShape {
        let options = [
            ConnectorShape::Square,
            ConnectorShape::Round,
            ConnectorShape::Round,
            ConnectorShape::Pointy,
            ConnectorShape::Pointy,
            ConnectorShape::Pointy,
        ];
        options[rng.gen_range(0..options.len())]
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BlockKind {
    Scaffold,
    Solid,
    /// Special blocks that hold the whole structure in place from the top
    Anchor,
}

impl BlockKind {
    pub fn get_texture(&self, textures: &Textures) -> Texture2D {
        match self {
            BlockKind::Scaffold => textures.scaffold,
            BlockKind::Solid => textures.solid,
            BlockKind::Anchor => textures.anchor,
        }
    }
}

impl Distribution<BlockKind> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BlockKind {
        let options = [BlockKind::Scaffold, BlockKind::Scaffold, BlockKind::Solid];
        options[rng.gen_range(0..options.len())].clone()
    }
}
