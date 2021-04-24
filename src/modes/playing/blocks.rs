use macroquad::prelude::Texture2D;
use quad_rand::compat::QuadRand;
use rand::Rng;

use super::BLOCK_SIZE;
use crate::{assets::Textures, Globals};

use cogs_gamedev::directions::Direction4;

#[derive(Clone)]
pub struct Block {
    /// Maps `Direction4 as usize` to the connector
    pub connectors: [Option<Connector>; 4],
    pub kind: BlockKind,
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

    pub fn draw_absolute(&self, cx: f32, cy: f32, globals: &Globals) {
        use macroquad::prelude::*;

        let tex = self.kind.get_texture(&globals.assets.textures);
        let corner_x = cx - BLOCK_SIZE / 2.0;
        let corner_y = cy - BLOCK_SIZE / 2.0;
        draw_texture(tex, corner_x, corner_y, WHITE);

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
                    WHITE,
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

#[derive(Clone)]
pub struct FallingBlock {
    pub block: Block,
    pub x: isize,
    pub y: f32,
}

#[derive(Clone)]
pub struct Connector {
    pub shape: ConnectorShape,
    pub sticks_out: bool,
}

impl Connector {
    pub fn sample() -> Self {
        Self {
            shape: ConnectorShape::sample(),
            sticks_out: QuadRand.gen_bool(0.5),
        }
    }

    pub fn links_with(&self, other: &Connector) -> bool {
        self.shape == other.shape && self.sticks_out != other.sticks_out
    }
}

/// The shape of the connector on the side of the block
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ConnectorShape {
    Square,
    Round,
    Pointy,
}

impl ConnectorShape {
    pub fn sample() -> Self {
        let options = [
            ConnectorShape::Square,
            ConnectorShape::Round,
            ConnectorShape::Pointy,
        ];
        options[QuadRand.gen_range(0..options.len())]
    }
}

#[derive(Clone, PartialEq, Eq)]
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
