
use std::sync::{Arc, Mutex};

use cpal::traits::StreamTrait;
use cpal::Stream;
use crossbeam_channel::{Receiver, Sender};
// use egui::ahash::{HashMap, HashMapExt};
use std::collections::HashMap;
use egui::load::SizedTexture;
use egui::{Color32, ColorImage, ImageData, ImageSource, Key, TextureHandle, TextureOptions};
use solgb::gameboy;
use solgb::gameboy::Gameboy;

use crate::audio::Audio;

pub const WIDTH: usize = gameboy::SCREEN_WIDTH as usize;
pub const HEIGHT: usize = gameboy::SCREEN_HEIGHT as usize;

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    #[serde(skip)]
    gameboy: Option<Gameboy>,
    #[serde(skip)]
    gb_texture: Option<TextureHandle>,
    #[serde(skip)]
    stream: Option<Stream>,
    #[serde(skip)]
    sender: Sender<(Vec<u8>, String)>,
    #[serde(skip)]
    receiver: Receiver<(Vec<u8>, String)>,
    save_ram: HashMap<String, Arc<Mutex<Vec<u8>>>>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();

        Self {
            gameboy: None,
            gb_texture: None,
            stream: None,
            sender,
            receiver,
            save_ram: HashMap::new(),
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

        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load(&mut self) {
        use std::fs::File;
        use std::io::Read;
        use rfd::FileDialog;

        let path = FileDialog::new()
            .add_filter("Gameboy Rom", &["gb", "gbc"])
            .add_filter("Gameboy Color Rom", &["gb", "gbc"])
            .set_directory("/")
            .pick_file().unwrap();

        let mut file = File::open(&path).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        self.sender.send(data).unwrap();
    }

    #[cfg(target_arch = "wasm32")]
    fn load(&mut self) {
        use rfd::AsyncFileDialog;

        let task = AsyncFileDialog::new()
            .add_filter("Gameboy Rom", &["gb", "gbc"])
            .add_filter("Gameboy Color Rom", &["gb", "gbc"])
            .set_directory("/")
            .pick_file();

        let sender = self.sender.clone();

        let future = async move {
            let file = task.await;    
            if let Some(file) = file {
                let data = file.read().await;
                sender.send((data, file.file_name())).unwrap();
            }
        };
        wasm_bindgen_futures::spawn_local(future);
    }

    fn setup(&mut self) {
        if let Ok((rom, name)) = self.receiver.try_recv() {

            let save_data = self.save_ram.entry(name).or_insert(Arc::new(Mutex::new(Vec::new())));
            // let save_data = Arc::new(Mutex::new(save_data.clone()));

            let mut gameboy = solgb::gameboy::GameboyBuilder::default()
            .with_rom(&rom)
            .with_model(Some(gameboy::GameboyType::CGB))
            .with_exram(save_data.clone())
            .build()
            .unwrap();

            let audio = Audio::new();
            let stream = audio.get_stream(gameboy.audio_control.clone());

            match gameboy.start() {
                Ok(_) => log::info!("Emulation started"),
                Err(error) => log::error!("Failed to start running emulation: {error}"),
            };

            self.gameboy = Some(gameboy);
            self.stream = Some(stream);

            // if let Some(stream) = &self.stream {
            //     stream.play().unwrap();
            // }
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

        self.setup();

        if let Some(gameboy) = &mut self.gameboy {
            if let Ok(buffer_u32) = gameboy.video_rec.try_recv() { // recv_timeout(Duration::new(0, 20000000)) {
                for _ in gameboy.video_rec.try_iter() {} //clear receive buffer "frame skip"
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
            
            //Update inputs
            ctx.input(|i| {
                let pressed = [
                    i.key_down(Key::Z),
                    i.key_down(Key::X),
                    i.key_down(Key::W),
                    i.key_down(Key::Enter),
                    i.key_down(Key::ArrowRight),
                    i.key_down(Key::ArrowLeft),
                    i.key_down(Key::ArrowUp),
                    i.key_down(Key::ArrowDown),
                ];
                gameboy.input_sender.send(pressed).unwrap();
            });
        }

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

                if ui.button("start").clicked() {
                    if let Some(stream) = &self.stream {
                        stream.play().unwrap();
                    }
                }

                if ui.button("open").clicked() {
                    self.load();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
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
