use egui::Color32;
use serde::{Deserialize, Serialize};

pub const PALETTES: [(&str, [[u8; 3]; 4]); 3] =
    [("Sandy", SANDY), ("Greyscale", GREYSCALE), ("Green", GREEN)];

pub const SANDY: [[u8; 3]; 4] = [
    [0xE6, 0xD6, 0x9C],
    [0xB4, 0xA5, 0x6A],
    [0x7B, 0x71, 0x62],
    [0x39, 0x38, 0x29],
];

pub const GREYSCALE: [[u8; 3]; 4] = [
    [0xFF, 0xFF, 0xFF],
    [0xAA, 0xAA, 0xAA],
    [0x55, 0x55, 0x55],
    [0x00, 0x00, 0x00],
];

pub const GREEN: [[u8; 3]; 4] = [
    [0xCA, 0xDC, 0x9F],
    [0x8B, 0xAC, 0x0F],
    [0x30, 0x62, 0x30],
    [0x0F, 0x38, 0x0F],
];

#[derive(Serialize, Deserialize)]
pub struct Palettes {
    pub bg: [[u8; 3]; 4],
    pub spr1: [[u8; 3]; 4],
    pub spr2: [[u8; 3]; 4],
    pub window_visible: bool,
}

impl Palettes {
    pub fn new() -> Self {
        Palettes {
            bg: SANDY,
            spr1: SANDY,
            spr2: SANDY,
            window_visible: false,
        }
    }

    pub fn draw_palette(&mut self, ui: &mut egui::Ui, name: &str, palette: &[[u8; 3]; 4]) -> bool {
        let mut changed = false;
        if ui.monospace(format!("{name: <16}")).clicked() {
            self.bg = *palette;
            self.spr1 = *palette;
            self.spr2 = *palette;
            changed = true;
        }
        for colors in palette {
            ui.colored_label(Color32::from_rgb(colors[0], colors[1], colors[2]), "⏹");
        }
        changed
    }

    pub fn get_u32_palette(&self) -> [[u32; 4]; 3] {
        [
            [
                u32::from_le_bytes([self.bg[0][2], self.bg[0][1], self.bg[0][0], 0xFF]),
                u32::from_le_bytes([self.bg[1][2], self.bg[1][1], self.bg[1][0], 0xFF]),
                u32::from_le_bytes([self.bg[2][2], self.bg[2][1], self.bg[2][0], 0xFF]),
                u32::from_le_bytes([self.bg[3][2], self.bg[3][1], self.bg[3][0], 0xFF]),
            ],
            [
                u32::from_le_bytes([self.spr1[0][2], self.spr1[0][1], self.spr1[0][0], 0xFF]),
                u32::from_le_bytes([self.spr1[1][2], self.spr1[1][1], self.spr1[1][0], 0xFF]),
                u32::from_le_bytes([self.spr1[2][2], self.spr1[2][1], self.spr1[2][0], 0xFF]),
                u32::from_le_bytes([self.spr1[3][2], self.spr1[3][1], self.spr1[3][0], 0xFF]),
            ],
            [
                u32::from_le_bytes([self.spr2[0][2], self.spr2[0][1], self.spr2[0][0], 0xFF]),
                u32::from_le_bytes([self.spr2[1][2], self.spr2[1][1], self.spr2[1][0], 0xFF]),
                u32::from_le_bytes([self.spr2[2][2], self.spr2[2][1], self.spr2[2][0], 0xFF]),
                u32::from_le_bytes([self.spr2[3][2], self.spr2[3][1], self.spr2[3][0], 0xFF]),
            ],
        ]
    }
}
