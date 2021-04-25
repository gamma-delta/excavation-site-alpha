#![allow(clippy::eval_order_dependence)]

use std::path::PathBuf;

use macroquad::{
    audio::{load_sound, Sound},
    prelude::{load_texture, FilterMode, Texture2D},
};
use once_cell::sync::Lazy;

#[derive(Clone)]
pub struct Assets {
    pub textures: Textures,
    pub sounds: Sounds,
}

impl Assets {
    pub async fn init() -> Self {
        Self {
            textures: Textures::init().await,
            sounds: Sounds::init().await,
        }
    }
}

#[derive(Clone)]
pub struct Textures {
    pub title_banner: Texture2D,
    pub title_screen: Texture2D,
    pub tutorial: Texture2D,

    pub scaffold: Texture2D,
    pub solid: Texture2D,
    pub anchor: Texture2D,
    pub connector_atlas: Texture2D,
    pub damage_atlas: Texture2D,

    pub dark_dirt: Texture2D,
    pub dirt_edge: Texture2D,
    pub dirt_body: Texture2D,

    pub conveyor: Texture2D,
    pub depth_meter: Texture2D,
    pub number_atlas: Texture2D,
}

impl Textures {
    async fn init() -> Self {
        Self {
            title_banner: texture("title/banner").await,
            title_screen: texture("titlescreen").await,
            tutorial: texture("tutorial").await,

            scaffold: texture("scaffold").await,
            solid: texture("rust2").await,
            anchor: texture("terrain-iron-simple-bottom").await,
            connector_atlas: texture("connector_atlas").await,
            damage_atlas: texture("damage_atlas").await,

            dark_dirt: texture("dirt").await,
            dirt_edge: texture("reinforced_dirt").await,
            dirt_body: texture("dirt_back").await,

            conveyor: texture("conveyor").await,
            depth_meter: texture("depth_meter").await,
            number_atlas: texture("number_atlas").await,
        }
    }
}

#[derive(Clone)]
pub struct Sounds {
    pub title_jingle: Sound,
    pub engineer_gaming: Sound,

    pub pickup: Sound,
    pub putdown: Sound,
    pub rotate: Sound,
    pub damage: Sound,
    pub fall: Sound,
}

impl Sounds {
    async fn init() -> Self {
        Self {
            title_jingle: sound("title/jingle").await,
            engineer_gaming: sound("engineer_gaming").await,

            pickup: sound("pick_up").await,
            putdown: sound("drop").await,
            rotate: sound("rotate").await,
            damage: sound("break").await,
            fall: sound("fall").await,
        }
    }
}

/// Path to the assets root
static ASSETS_ROOT: Lazy<PathBuf> = Lazy::new(|| {
    if cfg!(target_arch = "wasm32") {
        PathBuf::from("../assets")
    } else if cfg!(debug_assertions) {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets"))
    } else {
        todo!("assets path for release hasn't been finalized yet ;-;")
    }
});

async fn texture(path: &str) -> Texture2D {
    let with_extension = path.to_owned() + ".png";
    let tex = load_texture(
        ASSETS_ROOT
            .join("textures")
            .join(with_extension)
            .to_string_lossy()
            .as_ref(),
    )
    .await
    .unwrap();
    tex.set_filter(FilterMode::Nearest);
    tex
}

async fn sound(path: &str) -> Sound {
    let with_extension = path.to_owned() + ".ogg";
    load_sound(
        ASSETS_ROOT
            .join("sounds")
            .join(with_extension)
            .to_string_lossy()
            .as_ref(),
    )
    .await
    .unwrap()
}
