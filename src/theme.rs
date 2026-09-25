use std::fs;
use std::path::Path;

use ratatui::style::Color;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Theme {
    palette: Vec<Color>,
}

#[derive(Deserialize)]
struct ThemeFile {
    palette: Vec<Rgb>,
}

#[derive(Deserialize)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Theme {
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(path, include_str!("../config/colors.toml"));
        }
        let parsed = fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str::<ThemeFile>(&text).ok());
        let palette = parsed
            .map(|file| {
                file.palette
                    .into_iter()
                    .map(|rgb| Color::Rgb(rgb.r, rgb.g, rgb.b))
                    .collect()
            })
            .filter(|colors: &Vec<_>| !colors.is_empty())
            .unwrap_or_else(default_palette);
        Self { palette }
    }

    pub fn color(&self, index: u8) -> Color {
        let len = self.palette.len().max(1);
        self.palette[index as usize % len]
    }
}

fn default_palette() -> Vec<Color> {
    vec![
        Color::Rgb(80, 200, 255),
        Color::Rgb(255, 176, 46),
        Color::Rgb(126, 217, 87),
        Color::Rgb(255, 107, 129),
        Color::Rgb(179, 157, 255),
    ]
}
