
use std::sync::Arc;

use egui::load::SizedTexture;
use egui::{Color32, ColorImage, ImageData, ImageSource, TextureHandle, TextureOptions};
use solgb::gameboy;
use solgb::gameboy::Gameboy;

pub const WIDTH: usize = gameboy::SCREEN_WIDTH as usize;
pub const HEIGHT: usize = gameboy::SCREEN_HEIGHT as usize;

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,

    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,

    #[serde(skip)]
    gameboy: Gameboy,
    #[serde(skip)]
    gb_texture: Option<TextureHandle>,
}

impl Default for TemplateApp {
    fn default() -> Self {

        let rom = include_bytes!("D:\\Emulation\\TestRoms\\GB\\pocket.gb");
        let mut gameboy = solgb::gameboy::GameboyBuilder::default().with_rom(rom).build().unwrap();
        match gameboy.start() {
            _ => (),
        };

        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            gameboy,
            gb_texture: None,
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        let rom = include_bytes!("D:\\Emulation\\TestRoms\\GB\\pocket.gb");
        let mut gameboy = solgb::gameboy::GameboyBuilder::default().with_rom(rom).build().unwrap();
        match gameboy.start() {
            _ => (),
        };

        let color_image = Arc::new(ColorImage::new([WIDTH, HEIGHT], Color32::from_black_alpha(0)));
        let gb_image = ImageData::Color(color_image);

        let texutre_manager = cc.egui_ctx.tex_manager();
        let texture_id =
            texutre_manager
                .write()
                .alloc("genesis".into(), gb_image, TextureOptions::LINEAR);
        let gb_texture = Some(TextureHandle::new(texutre_manager, texture_id));

        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            gameboy,
            gb_texture,
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        if let Ok(buffer_u32) = self.gameboy.video_rec.try_recv() { // recv_timeout(Duration::new(0, 20000000)) {
            for _ in self.gameboy.video_rec.try_iter() {} //clear receive buffer "frame skip"
            if let Ok(buffer) = bytemuck::try_cast_slice(&buffer_u32) {
                let image = Arc::new(ColorImage {
                    size: [WIDTH, HEIGHT],
                    pixels: {
                        assert_eq!(WIDTH * HEIGHT * 4, buffer.len());
                        buffer
                            .chunks_exact(4)
                            .map(|p| Color32::from_rgba_premultiplied(p[2], p[1], p[0], p[3]))
                            .collect()
                    },
                });
                match &mut self.gb_texture {
                    Some(texture) => texture.set(image, TextureOptions::NEAREST),
                    None => {
                        let color_image = Arc::new(ColorImage::new([WIDTH, HEIGHT], Color32::from_black_alpha(0)));
                        let gb_image = ImageData::Color(color_image);

                        let texutre_manager = ctx.tex_manager();
                        let texture_id =
                            texutre_manager
                                .write()
                                .alloc("genesis".into(), gb_image, TextureOptions::LINEAR);
                        self.gb_texture = Some(TextureHandle::new(texutre_manager, texture_id));
                    }
                }
            }
        }
        self.gameboy.audio_control.dump_audio_buffer();

        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_dark_light_mode_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("eframe template");

            ui.horizontal(|ui| {
                ui.label("Write something: ");
                ui.text_edit_singleline(&mut self.label);
            });

            ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
            if ui.button("Increment").clicked() {
                self.value += 1.0;
            }

            ui.separator();

            ui.add(egui::github_link_file!(
                "https://github.com/emilk/eframe_template/blob/main/",
                "Source code."
            ));

            if let Some(gb_texture) = &self.gb_texture {
                ui.vertical_centered(|ui| {
                    let gameboy = egui::Image::new(ImageSource::Texture(SizedTexture::from_handle(
                        &gb_texture,
                    )))
                    .fit_to_fraction([1.0, 1.0].into());
                    ui.add(gameboy);
                });
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });

        ctx.request_repaint();
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
