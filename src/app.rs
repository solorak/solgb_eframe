use base64::{engine::general_purpose::STANDARD, Engine as _};
use cpal::traits::StreamTrait;
use cpal::Stream;
use egui::load::SizedTexture;
use egui::{Color32, ColorImage, ImageData, ImageSource, RichText, TextureHandle, TextureOptions};
use gilrs::Gilrs;
use serde::{Deserialize, Serialize};
use solgb::cart::CartType;
use solgb::gameboy::Gameboy;
use solgb::gameboy::{self, GameboyType};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::audio::Audio;
use crate::input::{Inputs, InputsState};
use crate::saves::Saves;

pub const WIDTH: usize = gameboy::SCREEN_WIDTH as usize;
pub const HEIGHT: usize = gameboy::SCREEN_HEIGHT as usize;

pub const DMG_ROM_NAME: &str = "_DMGBOOTROM";
pub const CGB_ROM_NAME: &str = "_CGBBOOTROM";

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
    last_save: Instant,
    #[serde(skip)]
    saves: Option<Saves>,
    #[serde(skip)]
    started: bool,
    #[serde(skip)]
    events: Events,
    save_manager_open: bool,
    volume: Volume,
    boot_rom_options: BootRomOptions,
    #[serde(skip)]
    inputs: Option<Inputs>,
    inputs_window_open: bool,
    input_state: InputsState,
    input_touch: [bool; 8],
    show_menu: bool,
    show_touch: bool,
}

impl Default for TemplateApp {
    fn default() -> Self {
        let events = Events::default();
        Self {
            gameboy: None,
            gb_texture: None,
            audio: Audio::new(),
            stream: None,
            last_save: Instant::now(),
            saves: Saves::new(events.clone()),
            started: false,
            events,
            save_manager_open: false,
            volume: Volume::default(),
            boot_rom_options: BootRomOptions::new(),
            inputs: None,
            inputs_window_open: false,
            input_state: InputsState::default(),
            input_touch: [false; 8],
            show_menu: true,
            show_touch: false,
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

        let mut style = (*cc.egui_ctx.style()).clone();
        for (_text_style, font_id) in style.text_styles.iter_mut() {
            font_id.size = 48.0 // whatever size you want here
        }
        cc.egui_ctx.set_style(style);

        egui_extras::install_image_loaders(&cc.egui_ctx);

        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load(&mut self) {
        use rfd::FileDialog;
        use std::fs::File;
        use std::io::Read;

        let path = FileDialog::new()
            .add_filter("Gameboy Rom", &["gb", "gbc"])
            .add_filter("Gameboy Color Rom", &["gb", "gbc"])
            .set_directory("/")
            .pick_file()
            .unwrap();

        let mut file = File::open(&path).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        self.sender.send(data).unwrap();
    }

    #[cfg(target_arch = "wasm32")]
    fn load(&mut self) {
        open(
            &self.events,
            &[
                (("Gameboy Rom"), &["gb", "gbc"]),
                ("Gameboy Color Rom", &["gb", "gbc"]),
            ],
            EventType::OpenRom,
        );
    }

    fn handle_custom_events(&mut self) {
        match self.events.get_next() {
            Some(Event::OpenRom(rom)) => {
                let (name, rom_type) = if let Ok(rom_info) = solgb::cart::RomInfo::new(&rom) {
                    (rom_info.get_name(), *rom_info.get_type())
                } else {
                    log::error!("ROM does not appear to be a gameboy game");
                    return;
                };

                if let Some(saves) = &mut self.saves {
                    saves.save_ram = if let Ok(Some(encoded)) = saves.storage.get_item(&name) {
                        let save_ram = STANDARD.decode(encoded).unwrap_or_default();
                        Arc::new(Mutex::new(save_ram))
                    } else {
                        Arc::new(Mutex::new(Vec::new()))
                    };

                    let mut boot_rom = match (&self.boot_rom_options.gb_type, &rom_type) {
                        (None, CartType::GB)
                        | (Some(GameboyType::DMG), CartType::GB)
                        | (Some(GameboyType::DMG), CartType::Hybrid)
                        | (Some(GameboyType::DMG), CartType::CGB) => {
                            let encoded = saves
                                .storage
                                .get_item(DMG_ROM_NAME)
                                .unwrap_or(None)
                                .unwrap_or_default();
                            STANDARD.decode(encoded).ok()
                        }
                        (None, CartType::CGB)
                        | (None, CartType::Hybrid)
                        | (Some(GameboyType::CGB), CartType::GB)
                        | (Some(GameboyType::CGB), CartType::CGB)
                        | (Some(GameboyType::CGB), CartType::Hybrid) => {
                            let encoded = saves
                                .storage
                                .get_item(CGB_ROM_NAME)
                                .unwrap_or(None)
                                .unwrap_or_default();
                            STANDARD.decode(encoded).ok()
                        }
                    };

                    if !self.boot_rom_options.use_bootrom {
                        boot_rom = None;
                    }

                    let mut gameboy = match solgb::gameboy::GameboyBuilder::default()
                        .with_rom(&rom)
                        .with_model(self.boot_rom_options.gb_type)
                        .with_exram(saves.save_ram.clone())
                        .with_boot_rom(boot_rom)
                        .build()
                    {
                        Ok(gameboy) => gameboy,
                        Err(err) => {
                            log::error!("Unable to setup gameboy: {err}");
                            saves.set_rom_info(None);
                            return;
                        }
                    };

                    self.audio.set_volume(self.volume.master as u8);
                    gameboy
                        .audio_control
                        .set_volume(gameboy::Channel::Square1, self.volume.square_1 as f32);
                    gameboy
                        .audio_control
                        .set_volume(gameboy::Channel::Square2, self.volume.square_2 as f32);
                    gameboy
                        .audio_control
                        .set_volume(gameboy::Channel::Wave, self.volume.wave as f32);
                    gameboy
                        .audio_control
                        .set_volume(gameboy::Channel::Noise, self.volume.noise as f32);

                    saves.set_rom_info(Some(gameboy.rom_info.clone()));

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

                    self.show_menu = false;
                }
            }
            Some(Event::SaveUpload(name, data)) => {
                if let Some(saves) = &mut self.saves {
                    saves.save(&name, &data);
                }
            }
            Some(Event::BootromUpload(br_type, data)) => {
                if let Some(saves) = &mut self.saves {
                    match br_type {
                        GameboyType::DMG => saves.save(DMG_ROM_NAME, &data),
                        GameboyType::CGB => saves.save(CGB_ROM_NAME, &data),
                    }
                }
            }
            _ => (),
        }
    }
 
    fn display_inputs(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let inputs = self.inputs.get_or_insert_with(|| {
            Inputs::with_state(Gilrs::new().unwrap(), ctx.clone(), self.input_state.clone())
        });
        ui.horizontal(|ui| {
            ui.monospace("A:        ".to_string());
            if ui
                .text_edit_singleline(&mut inputs.a.to_string())
                .has_focus()
            {
                inputs.update_buttons(crate::input::GBButton::A);
                self.input_state = inputs.save();
            }
        });
        ui.horizontal(|ui| {
            ui.monospace("B:        ".to_string());
            if ui
                .text_edit_singleline(&mut inputs.b.to_string())
                .has_focus()
            {
                inputs.update_buttons(crate::input::GBButton::B);
                self.input_state = inputs.save();
            }
        });
        ui.horizontal(|ui| {
            ui.monospace("Select:   ".to_string());
            if ui
                .text_edit_singleline(&mut inputs.select.to_string())
                .has_focus()
            {
                inputs.update_buttons(crate::input::GBButton::Select);
                self.input_state = inputs.save();
            }
        });
        ui.horizontal(|ui| {
            ui.monospace("Start:    ".to_string());
            if ui
                .text_edit_singleline(&mut inputs.start.to_string())
                .has_focus()
            {
                inputs.update_buttons(crate::input::GBButton::Start);
                self.input_state = inputs.save();
            }
        });
        ui.horizontal(|ui| {
            ui.monospace("Up:       ".to_string());
            if ui
                .text_edit_singleline(&mut inputs.up.to_string())
                .has_focus()
            {
                inputs.update_buttons(crate::input::GBButton::Up);
                self.input_state = inputs.save();
            }
        });
        ui.horizontal(|ui| {
            ui.monospace("Down:     ".to_string());
            if ui
                .text_edit_singleline(&mut inputs.down.to_string())
                .has_focus()
            {
                inputs.update_buttons(crate::input::GBButton::Down);
                self.input_state = inputs.save();
            }
        });
        ui.horizontal(|ui| {
            ui.monospace("Left:     ".to_string());
            if ui
                .text_edit_singleline(&mut inputs.left.to_string())
                .has_focus()
            {
                inputs.update_buttons(crate::input::GBButton::Left);
                self.input_state = inputs.save();
            }
        });
        ui.horizontal(|ui| {
            ui.monospace("Right:    ".to_string());
            if ui
                .text_edit_singleline(&mut inputs.right.to_string())
                .has_focus()
            {
                inputs.update_buttons(crate::input::GBButton::Right);
                self.input_state = inputs.save();
            }
        });
        
        ui.checkbox(&mut self.show_touch, "Show Touch Controls");
    }

    pub fn display_boot_roms(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.boot_rom_options.use_bootrom, "Use Bootrom");

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            ui.radio_value(&mut self.boot_rom_options.gb_type, None, "Auto");
            ui.radio_value(
                &mut self.boot_rom_options.gb_type,
                Some(GameboyType::DMG),
                "DMG",
            );
            ui.radio_value(
                &mut self.boot_rom_options.gb_type,
                Some(GameboyType::CGB),
                "CGB",
            );
        });

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            if ui.button("upload DMG").clicked() {
                open(
                    &self.events,
                    &[("Gameboy bootroom", &["bin", "rom"]), ("All Files", &["*"])],
                    EventType::BootromUpload(GameboyType::DMG),
                );
            }

            if ui.button("upload CGB").clicked() {
                open(
                    &self.events,
                    &[
                        ("Gameboy Color bootroom", &["bin", "rom"]),
                        ("All Files", &["*"]),
                    ],
                    EventType::BootromUpload(GameboyType::CGB),
                );
            }
        });
    }

    fn display_volume(&mut self, ui: &mut egui::Ui) {
        const VOLUME_RANGE: RangeInclusive<u32> = 0..=100;
        if ui
            .add(egui::Slider::new(&mut self.volume.master, VOLUME_RANGE).text("Master"))
            .changed()
        {
            self.audio.set_volume(self.volume.master as u8);
        };
        if ui
            .add(
                egui::Slider::new(&mut self.volume.square_1, VOLUME_RANGE).text("Square 1"),
            )
            .changed()
        {
            if let Some(gameboy) = &self.gameboy {
                gameboy
                    .audio_control
                    .set_volume(gameboy::Channel::Square1, self.volume.square_1 as f32)
            }
        };
        if ui
            .add(
                egui::Slider::new(&mut self.volume.square_2, VOLUME_RANGE).text("Square 2"),
            )
            .changed()
        {
            if let Some(gameboy) = &self.gameboy {
                gameboy
                    .audio_control
                    .set_volume(gameboy::Channel::Square2, self.volume.square_2 as f32)
            }
        };
        if ui
            .add(egui::Slider::new(&mut self.volume.wave, VOLUME_RANGE).text("Wave"))
            .changed()
        {
            if let Some(gameboy) = &self.gameboy {
                gameboy
                    .audio_control
                    .set_volume(gameboy::Channel::Wave, self.volume.wave as f32)
            }
        };
        if ui
            .add(egui::Slider::new(&mut self.volume.noise, VOLUME_RANGE).text("Noise"))
            .changed()
        {
            if let Some(gameboy) = &self.gameboy {
                gameboy
                    .audio_control
                    .set_volume(gameboy::Channel::Noise, self.volume.noise as f32)
            }
        };
    }
}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // //This does not work as expected in the browser
        // ctx.input(|i| {
        //     if let Some(stream) = &self.stream {
        //         for event in &i.raw.events {
        //             match event {
        //                 egui::Event::WindowFocused(focused) => {
        //                     if !focused {
        //                         log::error!("Window has lost focus, pausing");
        //                         stream.pause().unwrap();
        //                     } else {
        //                         log::error!("Window has gained focus, resuming");
        //                         stream.play().unwrap();
        //                     }
        //                 }
        //                 _ => (),
        //             }
        //         }
        //     }
        // });

        egui_extras::install_image_loaders(ctx);

        self.handle_custom_events();

        if let Some(saves) = &mut self.saves {
            if let Some(gameboy) = &self.gameboy {
                saves.save_current(&gameboy.rom_info.get_name());
            }
        }

        if let Some(gameboy) = &mut self.gameboy {
            if gameboy.video_rec.len() > 60 {
                log::warn!("We are over 1 second behind on rendering frames (was the window inactive?)\nskipping to current frame");
                while let Ok(_) = gameboy.video_rec.try_recv() {}
            }
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
                            let color_image = Arc::new(ColorImage::new(
                                [WIDTH, HEIGHT],
                                Color32::from_black_alpha(0),
                            ));
                            let gb_image = ImageData::Color(color_image);

                            let texutre_manager = ctx.tex_manager();
                            let texture_id = texutre_manager.write().alloc(
                                "genesis".into(),
                                gb_image,
                                TextureOptions::NEAREST,
                            );
                            self.gb_texture = Some(TextureHandle::new(texutre_manager, texture_id));
                        }
                    }
                }
            }

            //Update inputs
            let inputs = self.inputs.get_or_insert_with(|| {
                Inputs::with_state(Gilrs::new().unwrap(), ctx.clone(), self.input_state.clone())
            });
            while let Some(_event) = inputs.gilrs.next_event() {}
            let mut inputs = inputs.pressed_all();
            for i in 0..inputs.len() {
                if self.input_touch[i] {
                    inputs[i] = true;
                }
            }
            gameboy.input_sender.send(inputs).unwrap();
        }

        if self.show_menu {
            egui::Window::new("control panel")
            .fixed_pos([0.0, 0.0])
            .min_height(ctx.screen_rect().size().y)
            .min_width(400.0)
            .constrain(true)
            .title_bar(false)
            .resizable(true)
            .vscroll(true)
            .show(ctx, |ui| {
                const SPACE_BEFORE: f32 = 2.0;
                const SPACE_AFTER: f32 = 10.0;
                // ui.set_max_width(285.0);
                ui.set_min_height(ctx.screen_rect().size().y);

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

                if ui.button(RichText::new("≡").monospace()).clicked() {
                    self.show_menu = !self.show_menu;
                }

                egui::widgets::global_dark_light_mode_buttons(ui);

                let mut style = (*ctx.style()).clone();
                for (_text_style, font_id) in style.text_styles.iter_mut() {
                    font_id.size = 18.0 // whatever size you want here
                }
                ctx.set_style(style);

                if ui.add_sized(&[ui.available_width(), 0.0], egui::Button::new("open")).clicked() {
                    self.load();
                }

                if ui.add_sized(&[ui.available_width(), 0.0], egui::Button::new("bootroms")).clicked() {
                    self.boot_rom_options.window_visible = !self.boot_rom_options.window_visible;
                }

                if self.boot_rom_options.window_visible {
                    ui.add_space(SPACE_BEFORE);
                    self.display_boot_roms(ui);
                    ui.add_space(SPACE_AFTER);
                }

                if ui.add_sized(&[ui.available_width(), 0.0], egui::Button::new("saves")).clicked() {
                    self.save_manager_open = ! self.save_manager_open;
                }

                if self.save_manager_open {
                    ui.add_space(SPACE_BEFORE);
                    if let Some(saves) = &mut self.saves {
                        saves.show_save_manager(ui);
                    }
                    ui.add_space(SPACE_AFTER);
                }

                if ui.add_sized(&[ui.available_width(), 0.0], egui::Button::new("volume")).clicked() {
                    self.volume.window_visible = !self.volume.window_visible;
                }

                if self.volume.window_visible {
                    ui.add_space(SPACE_BEFORE);
                    self.display_volume(ui);
                    ui.add_space(SPACE_AFTER);
                }

                if ui.add_sized(&[ui.available_width(), 0.0], egui::Button::new("input")).clicked() {
                    self.inputs_window_open = !self.inputs_window_open;
                }

                if self.inputs_window_open {
                    ui.add_space(SPACE_BEFORE);
                    self.display_inputs(ctx, ui);
                    ui.add_space(SPACE_AFTER);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    powered_by_egui_and_eframe(ui);
                    egui::warn_if_debug_build(ui);
                });
            });
        } else {
            egui::Window::new("control panel")
            .fixed_pos([0.0, 0.0])
            .constrain(true)
            .title_bar(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.button(RichText::new("≡").monospace()).clicked() {
                    self.show_menu = !self.show_menu;
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(gb_texture) = &self.gb_texture {
                ui.vertical_centered(|ui| {
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
                        let gameboy = egui::Image::new(ImageSource::Texture(
                            SizedTexture::from_handle(gb_texture),
                        ))
                        .maintain_aspect_ratio(true)
                        .fit_to_fraction([1.0, 1.0].into());
                        ui.add(gameboy);
                    }
                });

                if self.show_touch {
                    ui.add_space(16.0);

                    ui.vertical_centered_justified(|ui| {
                        egui::Grid::new("touch_controls").spacing(&[0.0, 0.0]).min_col_width(ui.available_width() / 6.0).max_col_width(ui.available_width() / 6.0).show(ui, |ui| {
                            const A: usize = 0;
                            const B: usize = 1;
                            const RIGHT: usize = 4;
                            const LEFT: usize = 5;
                            const UP: usize = 6;
                            const DOWN: usize = 7;

                            let tile_size = [ui.available_width(), ui.available_width()];

                            self.input_touch = [false; 8];

                            let up_left = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/TRANS.png"))).contains_pointer();
                            let up = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/UP.png"))).contains_pointer();
                            let up_right = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/TRANS.png"))).contains_pointer();
                            ui.end_row();

                            let left = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/LEFT.png"))).contains_pointer();
                            ui.add_sized(&tile_size, egui::Label::new("")).contains_pointer();
                            let right = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/RIGHT.png"))).contains_pointer();
                            ui.add_sized(&tile_size, egui::Label::new("")).contains_pointer();
                            self.input_touch[B] = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/B.png"))).contains_pointer();
                            self.input_touch[A] = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/A.png"))).contains_pointer();
                            ui.end_row();

                            let down_left = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/TRANS.png"))).contains_pointer();
                            let down = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/DOWN.png"))).contains_pointer();
                            let down_right = ui.add_sized(&tile_size, egui::Image::new(egui::include_image!("../assets/TRANS.png"))).contains_pointer();
                            ui.end_row();
                            ui.end_row();

                            self.input_touch[UP] = up_left | up | up_right;
                            self.input_touch[DOWN] = down_left | down | down_right;
                            self.input_touch[LEFT] = up_left | left | down_left;
                            self.input_touch[RIGHT] = up_right | right | down_right;
                        });

                        ui.vertical_centered(|ui| {
                            const SELECT: usize = 2;
                            const START: usize = 3;
                            self.input_touch[SELECT] = ui.add_sized(&[ui.available_width(), 0.0], egui::Button::new("Select")).contains_pointer();
                            self.input_touch[START] = ui.add_sized(&[ui.available_width(), 0.0], egui::Button::new("Start")).contains_pointer();
                        });
                    });
                }
            }
        });

        ctx.request_repaint();
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(5)
    }
    
    fn persist_egui_memory(&self) -> bool {
        true
    }
    
    fn raw_input_hook(&mut self, _ctx: &egui::Context, _raw_input: &mut egui::RawInput) {}
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

#[derive(Serialize, Deserialize, Clone)]
struct Volume {
    pub master: u32,
    pub square_1: u32,
    pub square_2: u32,
    pub wave: u32,
    pub noise: u32,
    pub window_visible: bool,
}

impl Default for Volume {
    fn default() -> Self {
        Self {
            master: 100,
            square_1: 100,
            square_2: 100,
            wave: 100,
            noise: 100,
            window_visible: false,
        }
    }
}

//Bootrom
#[derive(Serialize, Deserialize)]
pub struct BootRomOptions {
    pub use_bootrom: bool,
    pub gb_type: Option<GameboyType>,
    pub window_visible: bool,
}

impl BootRomOptions {
    pub fn new() -> Self {
        Self {
            use_bootrom: false,
            gb_type: None,
            window_visible: false,
        }
    }
}

#[derive(Clone)]
pub struct Events(Rc<RefCell<VecDeque<Event>>>);

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
    OpenRom(Vec<u8>),
    SaveUpload(String, Vec<u8>),
    BootromUpload(GameboyType, Vec<u8>),
}

#[derive(Copy, Clone)]
pub(crate) enum EventType {
    OpenRom,
    SaveUpload,
    BootromUpload(GameboyType),
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn open(events: &Events, filter: &[(&str, &[&str])], event_type: EventType) {
    use rfd::AsyncFileDialog;

    hide_canvas();

    let mut file_dialog = AsyncFileDialog::new();
    for (name, extensions) in filter {
        file_dialog = file_dialog.add_filter(*name, extensions);
    }
    file_dialog = file_dialog.set_directory("/");
    let task = file_dialog.pick_file();

    let events = events.clone();

    let future = async move {
        let file = task.await;
        if let Some(file) = file {
            let data = file.read().await;
            match event_type {
                EventType::OpenRom => events.push(Event::OpenRom(data)),
                EventType::SaveUpload => events.push(Event::SaveUpload(file.file_name(), data)),
                EventType::BootromUpload(gb_type) => {
                    events.push(Event::BootromUpload(gb_type, data))
                }
            }
        }
        show_canvas()
    };

    wasm_bindgen_futures::spawn_local(future);
}

//We have to hide the canvas while opening files because in some browsers the buttons don't work
fn hide_canvas() {
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("the_canvas_id"));
    if let Some(canvas) = canvas {
        canvas
            .set_attribute("style", "outline: none; display: none;")
            .unwrap();
    }
}

fn show_canvas() {
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("the_canvas_id"));
    if let Some(canvas) = canvas {
        canvas.set_attribute("style", "outline: none;").unwrap();
    }
}
