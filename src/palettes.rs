use egui::{
    ahash::{HashMap, HashMapExt},
    Color32,
};
use serde::{Deserialize, Serialize};

pub const PALETTES: [(&str, [[u8; 3]; 4]); 4] = [
    ("Sandy", SANDY),
    ("Greyscale", GREYSCALE),
    ("Green", GREEN),
    ("Blue", BLUE),
];

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

pub const BLUE: [[u8; 3]; 4] = [
    [0x4A, 0xB1, 0xD8],
    [0x57, 0x7C, 0xBC],
    [0x52, 0x56, 0xBC],
    [0x3A, 0x3E, 0x98],
];

#[derive(Serialize, Deserialize)]
pub struct Palettes {
    pub bg: [[u8; 3]; 4],
    pub spr1: [[u8; 3]; 4],
    pub spr2: [[u8; 3]; 4],
    pub window_visible: bool,
    pub custom_name: String,
    multi_palette: bool,
    custom_palettes: HashMap<String, [[[u8; 3]; 4]; 3]>,
}

impl Palettes {
    pub fn new() -> Self {
        Palettes {
            bg: SANDY,
            spr1: SANDY,
            spr2: SANDY,
            window_visible: false,
            custom_name: String::from("custom"),
            multi_palette: false,
            custom_palettes: HashMap::new(),
        }
    }

    pub fn display_palettes(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        ui.text_edit_singleline(&mut self.custom_name);

        if ui
            .checkbox(&mut self.multi_palette, "Multi Palette")
            .changed()
        {
            changed |= true;
        }

        if self.multi_palette {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.monospace("Background:     ");
                for palette in &mut self.bg {
                    changed |= ui.color_edit_button_srgb(palette).changed()
                }
            });

            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.monospace("Sprite Layer 1: ");
                for palette in &mut self.spr1 {
                    changed |= ui.color_edit_button_srgb(palette).changed()
                }
            });

            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.monospace("Sprite Layer 2: ");
                for palette in &mut self.spr2 {
                    changed |= ui.color_edit_button_srgb(palette).changed()
                }
            });
        } else {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.monospace("Palette:        ");
                for palette in &mut self.bg {
                    changed |= ui.color_edit_button_srgb(palette).changed()
                }
            });
        }

        if ui.button("Save").clicked() {
            self.save_palette();
        }

        ui.monospace("Default Palettes");

        for (name, palette) in PALETTES {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                changed |= self.update_palettes(ui, name, &[palette, palette, palette]);
            });
        }

        if !self.custom_palettes.is_empty() {
            ui.monospace("Custom Palettes");
        }

        for (name, palette) in &self.custom_palettes.clone() {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                changed |= self.update_palettes(ui, &name, &palette);
                if ui.button("x").clicked() {
                    self.custom_palettes.remove(name);
                }
            });
        }

        changed
    }

    fn update_palettes(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        palette: &[[[u8; 3]; 4]; 3],
    ) -> bool {
        let mut changed = false;
        if ui.monospace(format!("{name: <16}")).clicked() {
            if self.multi_palette {
                self.bg = palette[0];
                self.spr1 = palette[1];
                self.spr2 = palette[2];
            } else {
                self.bg = palette[0];
                self.spr1 = palette[0];
                self.spr2 = palette[0];
            }
            self.custom_name = name.into();
            changed = true;
        }
        for colors in palette[0] {
            ui.colored_label(Color32::from_rgb(colors[0], colors[1], colors[2]), "⏹");
        }
        changed
    }

    fn save_palette(&mut self) {
        if self.multi_palette {
            self.custom_palettes
                .insert(self.custom_name.clone(), [self.bg, self.spr1, self.spr2]);
        } else {
            self.custom_palettes
                .insert(self.custom_name.clone(), [self.bg, self.bg, self.bg]);
        }
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
