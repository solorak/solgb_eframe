
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use cpal::traits::StreamTrait;
use cpal::Stream;
use egui::load::SizedTexture;
use egui::{Color32, ColorImage, ImageData, ImageSource, Key, TextureHandle, TextureOptions};
use gilrs::{Button, Gilrs};
use solgb::gameboy;
use solgb::gameboy::Gameboy;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::audio::Audio;
use crate::saves::Saves;

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
    audio: Audio,
    #[serde(skip)]
    stream: Option<Stream>,
    #[serde(skip)]
    gilrs: Gilrs,
    #[serde(skip)]
    last_save: Instant,
    #[serde(skip)]
    saves: Option<Saves>,
    #[serde(skip)]
    started: bool,
    #[serde(skip)]
    events: Events,
    save_manager_open: bool,
}

impl Default for TemplateApp {
    fn default() -> Self {
        let events = Events::default();
        Self {
            gameboy: None,
            gb_texture: None,
            audio: Audio::new(),
            stream: None,
            gilrs: Gilrs::new().unwrap(),
            last_save: Instant::now(),
            saves: Saves::new(events.clone()),
            started: false,
            events,
            save_manager_open: true,
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

        let events = self.events.clone();

        let future = async move {
            let file = task.await;    
            if let Some(file) = file {
                let data = file.read().await;
                events.push(Event::OpenRom(file.file_name(), data));
            }
        };
        wasm_bindgen_futures::spawn_local(future);
    }

    fn setup(&mut self) {
        match self.events.get_next() {
            Some(Event::OpenRom(name, rom)) => {

                let name = if let Ok(rom_info) = solgb::cart::RomInfo::new(&rom) {
                    rom_info.get_name()
                } else {
                    log::error!("ROM does not appear to be a gameboy game");
                    return
                };

                if let Some(saves) = &mut self.saves {
                    saves.save_ram = if let Ok(Some(encoded)) = saves.storage.get_item(&name) {
                        let save_ram = STANDARD.decode(encoded).unwrap_or_default();
                        Arc::new(Mutex::new(save_ram))
                    } else {
                        Arc::new(Mutex::new(Vec::new()))
                    };

                    // self.current_name = Some(name);

                    let mut gameboy = solgb::gameboy::GameboyBuilder::default()
                    .with_rom(&rom)
                    .with_model(Some(gameboy::GameboyType::CGB))
                    .with_exram(saves.save_ram.clone())
                    .build()
                    .unwrap();

                    if let Some(stream) = &self.stream {
                        if let Err(error) = stream.pause() {
                            log::warn!("Unable to pause stream: {error}");
                        }
                    }

                    let stream = self.audio.get_stream(gameboy.audio_control.clone());

                    match gameboy.start() {
                        Ok(_) => log::info!("Emulation started"),
                        Err(error) => log::error!("Failed to start running emulation: {error}"),
                    };

                    self.gameboy = Some(gameboy);
                    self.stream = Some(stream);

                    self.started = false;
                }
            }
            Some(Event::SaveUpload(name, data)) => {
                if let Some(saves) = &mut self.saves {
                    saves.save(&name, &data);
                }
            }
            _ => (),
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

        if let Some(saves) = &mut self.saves {
            if let Some(gameboy) = &self.gameboy {
                saves.save_current(&gameboy.rom_info.get_name());
            }
        }

        if let Some(gameboy) = &mut self.gameboy {
            if let Ok(buffer_u32) = gameboy.video_rec.try_recv() {
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
            let mut inputs = [false; 8];

            ctx.input(|i| {
                inputs = [
                    i.key_down(Key::Z),
                    i.key_down(Key::X),
                    i.key_down(Key::W),
                    i.key_down(Key::Enter),
                    i.key_down(Key::ArrowRight),
                    i.key_down(Key::ArrowLeft),
                    i.key_down(Key::ArrowUp),
                    i.key_down(Key::ArrowDown),
                ];
            });

            while let Some(_event) = self.gilrs.next_event() {}
            for (_id, gamepad) in self.gilrs.gamepads() {
                log::info!("{}", gamepad.name());

                inputs[0] = gamepad.is_pressed(Button::South);
                inputs[1] = gamepad.is_pressed(Button::West);
                inputs[2] = gamepad.is_pressed(Button::Select);
                inputs[3] = gamepad.is_pressed(Button::Start);
                inputs[4] = gamepad.is_pressed(Button::DPadRight);
                inputs[5] = gamepad.is_pressed(Button::DPadLeft);
                inputs[6] = gamepad.is_pressed(Button::DPadUp);
                inputs[7] = gamepad.is_pressed(Button::DPadDown);
            }

            gameboy.input_sender.send(inputs).unwrap();
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

                if ui.button("open").clicked() {
                    self.load();
                }

                if ui.button("save manager").clicked() {
                    self.save_manager_open = true;
                    ui.close_menu();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {

            if let Some(gb_texture) = &self.gb_texture {
                ui.centered_and_justified(|ui| {
                    if !self.started {
                        if ui.button("start").clicked() {
                            if let Some(stream) = &self.stream {
                                if let Err(error) = stream.play() {
                                    log::warn!("Unable to start stream: {error}");
                                }
                            }
                            self.started = true;
                        }
                    } else {
                        let gameboy = egui::Image::new(ImageSource::Texture(SizedTexture::from_handle(
                            &gb_texture,
                        )))
                        .fit_to_fraction([1.0, 1.0].into());
                        ui.add(gameboy);
                    }
                });
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });

        egui::Window::new("Save Manager")
            .title_bar(true)
            .resizable(false)
            .collapsible(false)
            .open(&mut self.save_manager_open)
            .show(ctx, |ui| {
                if let Some(saves) = &mut self.saves {
                    saves.show_save_manager(ui);
                }
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

#[derive(Clone)]
pub struct Events (Rc<RefCell<VecDeque<Event>>>);

impl Events {
    pub fn get_next(&self) -> Option<Event> {
        self.0.borrow_mut().pop_front()
    }

    pub fn push(&self, event: Event) {
        self.0.borrow_mut().push_back(event)
    }
}

impl Default for Events {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(VecDeque::new())))
    }
}

pub enum Event {
    OpenRom(String, Vec<u8>),
    SaveUpload(String, Vec<u8>),
}