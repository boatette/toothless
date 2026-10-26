use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, Layer};

use crate::pet::Thresholds;

const CONFIG_RELATIVE_PATH: &str = "toothless/config.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub fn anchor(self) -> Anchor {
        match self {
            Corner::TopLeft => Anchor::TOP | Anchor::LEFT,
            Corner::TopRight => Anchor::TOP | Anchor::RIGHT,
            Corner::BottomLeft => Anchor::BOTTOM | Anchor::LEFT,
            Corner::BottomRight => Anchor::BOTTOM | Anchor::RIGHT,
        }
    }

    pub fn is_right(self) -> bool {
        matches!(self, Corner::TopRight | Corner::BottomRight)
    }

    pub fn is_top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub corner: Corner,
    /// Gap from the screen edges, in logical pixels.
    pub margin: i32,
    /// Gap from the left or right edge; takes `margin` when unset.
    pub margin_x: Option<i32>,
    /// Gap from the top or bottom edge; takes `margin` when unset.
    pub margin_y: Option<i32>,
    /// Screen pixels per sprite pixel.
    pub scale: u32,
    /// Output to live on, by connector name; the first one to appear when unset.
    pub output: Option<String>,
    #[serde(deserialize_with = "deserialize_layer")]
    pub layer: Layer,
    /// Face left instead of right. Unset, he faces the middle of the screen.
    pub mirror: Option<bool>,
    pub awake: f32,
    pub excited: f32,
    pub charging: f32,
    /// Milliseconds between CPU readings.
    pub sample_interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            corner: Corner::BottomRight,
            margin: 24,
            margin_x: None,
            margin_y: None,
            scale: 4,
            output: None,
            layer: Layer::Bottom,
            mirror: None,
            awake: 10.0,
            excited: 50.0,
            charging: 85.0,
            sample_interval: 2000,
        }
    }
}

fn deserialize_layer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Layer, D::Error> {
    let text = String::deserialize(deserializer)?;
    match text.as_str() {
        "background" => Ok(Layer::Background),
        "bottom" => Ok(Layer::Bottom),
        "top" => Ok(Layer::Top),
        "overlay" => Ok(Layer::Overlay),
        other => Err(serde::de::Error::custom(format!(
            "`{other}` is not a layer, expected background, bottom, top or overlay"
        ))),
    }
}

impl Config {
    /// Gap from the left or right screen edge.
    pub fn horizontal_margin(&self) -> i32 {
        self.margin_x.unwrap_or(self.margin)
    }

    /// Gap from the top or bottom screen edge.
    pub fn vertical_margin(&self) -> i32 {
        self.margin_y.unwrap_or(self.margin)
    }

    pub fn load() -> Result<Self> {
        let Some(path) = config_path() else {
            return Ok(Self::default());
        };
        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).with_context(|| format!("in {}", path.display())),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).with_context(|| format!("could not read {}", path.display())),
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text)?;
        if !(1..=16).contains(&config.scale) {
            bail!("scale must be between 1 and 16");
        }
        if !(config.awake <= config.excited && config.excited <= config.charging) {
            bail!("thresholds must rise: awake <= excited <= charging");
        }
        if config.sample_interval < 250 {
            bail!("sample_interval must be at least 250 milliseconds");
        }
        Ok(config)
    }

    pub fn thresholds(&self) -> Thresholds {
        Thresholds {
            awake: self.awake,
            excited: self.excited,
            charging: self.charging,
        }
    }

    pub fn mirrored(&self) -> bool {
        self.mirror.unwrap_or(self.corner.is_right())
    }
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join(CONFIG_RELATIVE_PATH))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_is_the_defaults() {
        let config = Config::parse("").unwrap();
        assert_eq!(Corner::BottomRight, config.corner);
        assert!(config.mirrored());
    }

    #[test]
    fn reads_every_key() {
        let config = Config::parse(
            r#"
            corner = "top-left"
            margin = 8
            margin_x = 12
            margin_y = 4
            scale = 3
            output = "DP-1"
            layer = "top"
            mirror = true
            awake = 5
            excited = 40
            charging = 90
            sample_interval = 1000
            "#,
        )
        .unwrap();
        assert_eq!(Corner::TopLeft, config.corner);
        assert_eq!(
            (12, 4),
            (config.horizontal_margin(), config.vertical_margin())
        );
        assert_eq!(Some("DP-1"), config.output.as_deref());
        assert_eq!(Layer::Top, config.layer);
        assert!(config.mirrored());
    }

    #[test]
    fn axis_margins_fall_back_to_margin() {
        let config = Config::parse("margin = 8\nmargin_y = 2").unwrap();
        assert_eq!(
            (8, 2),
            (config.horizontal_margin(), config.vertical_margin())
        );
    }

    #[test]
    fn rejects_nonsense() {
        assert!(Config::parse("scale = 0").is_err());
        assert!(Config::parse("awake = 60").is_err());
        assert!(Config::parse("colour = \"black\"").is_err());
    }
}
