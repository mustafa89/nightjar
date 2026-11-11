//! Skins: every colour the deck, playlist and stage draw with, plus the visualizer's palette.
//! Switching skins moves every colour on a spring.

use rune::Color;

pub struct Skin {
    pub name: &'static str,
    pub dark: bool,
    /// Window background, top to bottom.
    pub bg: [&'static str; 2],
    /// Card faces, top to bottom, and their bevel (light top edge, dark bottom edge).
    pub panel: [&'static str; 2],
    pub bevel: [&'static str; 2],
    /// The display window: glass, lit text and unlit segments.
    pub lcd: &'static str,
    pub lcd_text: &'static str,
    pub lcd_dim: &'static str,
    pub accent: &'static str,
    pub accent2: &'static str,
    pub text: &'static str,
    pub muted: &'static str,
    /// Spectrum bars, top to bottom.
    pub bars: [&'static str; 2],
    /// Button faces, top to bottom.
    pub button: [&'static str; 2],
    /// The visualizer's three palette colours.
    pub viz: [&'static str; 3],
}

pub const SKINS: &[Skin] = &[
    Skin {
        name: "Midnight Chrome",
        dark: true,
        bg: ["#0a0f1f", "#141c38"],
        panel: ["#1f2a4d", "#10172d"],
        bevel: ["#4a5c94", "#05070f"],
        lcd: "#040912",
        lcd_text: "#7cf3ff",
        lcd_dim: "#173a44",
        accent: "#29d3ff",
        accent2: "#b16cff",
        text: "#e6ecff",
        muted: "#8592bd",
        bars: ["#ff4fd8", "#2a6bff"],
        button: ["#34416e", "#182040"],
        viz: ["#29d3ff", "#b16cff", "#ff4fd8"],
    },
    Skin {
        name: "Ember Vinyl",
        dark: true,
        bg: ["#160a07", "#2b130c"],
        panel: ["#40200f", "#211009"],
        bevel: ["#8a4a24", "#0e0503"],
        lcd: "#120604",
        lcd_text: "#ffb347",
        lcd_dim: "#4a2a12",
        accent: "#ff7a2f",
        accent2: "#ffd166",
        text: "#fff1e4",
        muted: "#c08f6c",
        bars: ["#ffe066", "#ff3d2e"],
        button: ["#5a2d16", "#2c140a"],
        viz: ["#ff3d2e", "#ff9f1c", "#ffe066"],
    },
    Skin {
        name: "Phosphor",
        dark: true,
        bg: ["#030805", "#08140c"],
        panel: ["#11221a", "#07110b"],
        bevel: ["#2f5a3c", "#010302"],
        lcd: "#010d04",
        lcd_text: "#6dff8a",
        lcd_dim: "#123a1b",
        accent: "#39ff6a",
        accent2: "#c6ff3d",
        text: "#dcffe4",
        muted: "#6fa47c",
        bars: ["#d4ff5a", "#1fbf4a"],
        button: ["#1c3626", "#0a1610"],
        viz: ["#0b5d1e", "#39ff6a", "#d4ff5a"],
    },
    Skin {
        name: "Candy Pop",
        dark: false,
        bg: ["#ffd6ec", "#d9e2ff"],
        panel: ["#ffffff", "#f1ecfc"],
        bevel: ["#ffffff", "#c4bce3"],
        lcd: "#2b2440",
        lcd_text: "#ffd9ef",
        lcd_dim: "#4a3f66",
        accent: "#ff4fa3",
        accent2: "#6c7bff",
        text: "#2b2440",
        muted: "#857aa3",
        bars: ["#ff4fa3", "#6c7bff"],
        button: ["#ffffff", "#e6e0f7"],
        viz: ["#ff4fa3", "#6c7bff", "#3ee6c5"],
    },
];

pub fn c(hex: &str) -> Color {
    rune::color::hex(hex).unwrap_or(Color::BLACK)
}

/// A colour for shaders, in linear light (`rune::hex` decodes sRGB).
pub fn rgb(hex: &str) -> [f32; 4] {
    let c = c(hex);
    [c.r, c.g, c.b, 1.0]
}

pub fn skin(i: usize) -> &'static Skin {
    &SKINS[i % SKINS.len()]
}
