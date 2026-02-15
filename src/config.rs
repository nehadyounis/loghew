use std::path::PathBuf;

use ratatui::style::Color;
use serde::Deserialize;

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ConfigFile {
    pub general: GeneralConfig,
    pub colors: ColorConfig,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub scroll_speed: usize,
    pub line_numbers: bool,
    pub mouse: bool,
    pub theme: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            scroll_speed: 3,
            line_numbers: true,
            mouse: true,
            theme: "default".to_string(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct ColorConfig {
    pub error: String,
    pub warn: String,
    pub info: String,
    pub debug: String,
    pub trace: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            error: String::new(),
            warn: String::new(),
            info: String::new(),
            debug: String::new(),
            trace: String::new(),
        }
    }
}

pub struct ColorPreset {
    pub name: &'static str,
    pub error: Color,
    pub warn: Color,
    pub info: Color,
    pub debug: Color,
    pub trace: Color,
}

pub const PRESETS: &[ColorPreset] = &[
    ColorPreset {
        name: "default",
        error: Color::LightRed,
        warn: Color::Yellow,
        info: Color::Reset,
        debug: Color::Cyan,
        trace: Color::DarkGray,
    },
    ColorPreset {
        name: "dracula",
        error: Color::Rgb(255, 85, 85),   // red
        warn: Color::Rgb(255, 184, 108),   // orange
        info: Color::Rgb(139, 233, 253),   // cyan
        debug: Color::Rgb(189, 147, 249),  // purple
        trace: Color::Rgb(98, 114, 164),   // comment gray
    },
    ColorPreset {
        name: "nord",
        error: Color::Rgb(191, 97, 106),   // aurora red
        warn: Color::Rgb(235, 203, 139),   // aurora yellow
        info: Color::Rgb(136, 192, 208),   // frost blue
        debug: Color::Rgb(180, 142, 173),  // aurora purple
        trace: Color::Rgb(76, 86, 106),    // polar night
    },
    ColorPreset {
        name: "gruvbox",
        error: Color::Rgb(251, 73, 52),    // red
        warn: Color::Rgb(250, 189, 47),    // yellow
        info: Color::Rgb(184, 187, 38),    // green
        debug: Color::Rgb(131, 165, 152),  // aqua
        trace: Color::Rgb(146, 131, 116),  // gray
    },
    ColorPreset {
        name: "monokai",
        error: Color::Rgb(249, 38, 114),   // pink
        warn: Color::Rgb(230, 219, 116),   // yellow
        info: Color::Rgb(102, 217, 239),   // cyan
        debug: Color::Rgb(166, 226, 46),   // green
        trace: Color::Rgb(117, 113, 94),   // comment gray
    },
    ColorPreset {
        name: "solarized",
        error: Color::Rgb(220, 50, 47),    // red
        warn: Color::Rgb(181, 137, 0),     // yellow
        info: Color::Rgb(42, 161, 152),    // cyan
        debug: Color::Rgb(38, 139, 210),   // blue
        trace: Color::Rgb(88, 110, 117),   // base01
    },
    ColorPreset {
        name: "catppuccin",
        error: Color::Rgb(243, 139, 168),  // red
        warn: Color::Rgb(250, 179, 135),   // peach
        info: Color::Rgb(137, 180, 250),   // blue
        debug: Color::Rgb(180, 190, 254),  // lavender
        trace: Color::Rgb(108, 112, 134),  // overlay
    },
    ColorPreset {
        name: "tokyo night",
        error: Color::Rgb(247, 118, 142),  // red
        warn: Color::Rgb(224, 175, 104),   // yellow
        info: Color::Rgb(122, 162, 247),   // blue
        debug: Color::Rgb(158, 206, 106),  // green
        trace: Color::Rgb(86, 95, 137),    // comment
    },
];

pub struct Config {
    pub scroll_speed: usize,
    pub line_numbers: bool,
    pub mouse: bool,
    pub theme_index: usize,
    pub error_color: Color,
    pub warn_color: Color,
    pub info_color: Color,
    pub debug_color: Color,
    pub trace_color: Color,
}

impl Config {
    pub fn load() -> Self {
        let file_config = config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str::<ConfigFile>(&s).ok())
            .unwrap_or_default();

        let theme_index = PRESETS
            .iter()
            .position(|p| p.name == file_config.general.theme)
            .unwrap_or(0);

        let preset = &PRESETS[theme_index];

        let error_color = compat_color(parse_color_or(&file_config.colors.error, preset.error));
        let warn_color = compat_color(parse_color_or(&file_config.colors.warn, preset.warn));
        let info_color = compat_color(parse_color_or(&file_config.colors.info, preset.info));
        let debug_color = compat_color(parse_color_or(&file_config.colors.debug, preset.debug));
        let trace_color = compat_color(parse_color_or(&file_config.colors.trace, preset.trace));

        Self {
            scroll_speed: file_config.general.scroll_speed.max(1),
            line_numbers: file_config.general.line_numbers,
            mouse: file_config.general.mouse,
            theme_index,
            error_color,
            warn_color,
            info_color,
            debug_color,
            trace_color,
        }
    }

    pub fn apply_preset(&mut self, index: usize) {
        self.theme_index = index;
        let preset = &PRESETS[index];
        self.error_color = compat_color(preset.error);
        self.warn_color = compat_color(preset.warn);
        self.info_color = compat_color(preset.info);
        self.debug_color = compat_color(preset.debug);
        self.trace_color = compat_color(preset.trace);
    }

    pub fn theme_name(&self) -> &'static str {
        PRESETS[self.theme_index].name
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("loghew").join("config.toml"))
}

fn supports_truecolor() -> bool {
    if let Ok(val) = std::env::var("COLORTERM") {
        let v = val.to_lowercase();
        return v == "truecolor" || v == "24bit";
    }
    false
}

static TRUECOLOR: std::sync::LazyLock<bool> = std::sync::LazyLock::new(supports_truecolor);

fn compat_color(c: Color) -> Color {
    if let Color::Rgb(r, g, b) = c {
        if !*TRUECOLOR {
            return Color::Indexed(rgb_to_index(r, g, b));
        }
    }
    c
}

fn rgb_to_index(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let nearest = |v: u8| -> u8 {
        let mut best = 0;
        let mut best_d = 255u16;
        for (i, &l) in LEVELS.iter().enumerate() {
            let d = (v as i16 - l as i16).unsigned_abs();
            if d < best_d {
                best_d = d;
                best = i as u8;
            }
        }
        best
    };
    let ri = nearest(r);
    let gi = nearest(g);
    let bi = nearest(b);

    let gray_avg = (r as u16 + g as u16 + b as u16) / 3;
    let gray_idx = if gray_avg <= 4 {
        0
    } else if gray_avg >= 248 {
        23
    } else {
        ((gray_avg - 8) / 10) as u8
    };
    let gray_val = 8 + 10 * gray_idx as u16;
    let gray_dist = (r as i16 - gray_val as i16).unsigned_abs()
        + (g as i16 - gray_val as i16).unsigned_abs()
        + (b as i16 - gray_val as i16).unsigned_abs();

    let cube_r = LEVELS[ri as usize];
    let cube_g = LEVELS[gi as usize];
    let cube_b = LEVELS[bi as usize];
    let cube_dist = (r as i16 - cube_r as i16).unsigned_abs()
        + (g as i16 - cube_g as i16).unsigned_abs()
        + (b as i16 - cube_b as i16).unsigned_abs();

    if gray_dist < cube_dist {
        232 + gray_idx
    } else {
        16 + 36 * ri + 6 * gi + bi
    }
}

fn parse_color_or(s: &str, fallback: Color) -> Color {
    if s.is_empty() {
        return fallback;
    }
    parse_color(s, fallback)
}

fn parse_color(s: &str, fallback: Color) -> Color {
    let s = s.trim().to_lowercase();
    let hex = s.strip_prefix('#').unwrap_or(&s);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        return Color::Rgb(r, g, b);
    }
    match s.as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark_gray" | "dark_grey" | "darkgray" => Color::DarkGray,
        "light_red" | "lightred" => Color::LightRed,
        "light_green" | "lightgreen" => Color::LightGreen,
        "light_yellow" | "lightyellow" => Color::LightYellow,
        "light_blue" | "lightblue" => Color::LightBlue,
        "light_magenta" | "lightmagenta" => Color::LightMagenta,
        "light_cyan" | "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        "reset" | "default" => Color::Reset,
        _ => fallback,
    }
}
