use base64::{engine::general_purpose::STANDARD, Engine as _};
use solgb::{
    cart::{CartType, RomInfo},
    gameboy::GameboyType,
};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{Arc, Mutex},
};
use wasm_bindgen::JsCast;
use web_sys::Storage;
use web_time::{Duration, Instant};
use zip::write::SimpleFileOptions;

use crate::app::{BootRomOptions, Events, CGB_ROM_NAME, DMG_ROM_NAME};

pub struct Saves {
    storage: Storage,
    last_save: Instant,
    pub save_ram: Arc<Mutex<Vec<u8>>>,
    events: Events,
    save_data: BTreeMap<String, (String, String)>,
    rom_info: Option<RomInfo>,
}

impl Saves {
    pub fn new(events: Events) -> Option<Self> {
        let Some(Some(storage)) = web_sys::window().and_then(|s| s.local_storage().ok()) else {
            return None;
        };
        Some(Self {
            storage,
            last_save: Instant::now(),
            save_ram: Arc::new(Mutex::new(Vec::new())),
            events,
            save_data: BTreeMap::default(),
            rom_info: None,
        })
    }

    pub fn set_rom_info(&mut self, rom_info: Option<RomInfo>) {
        self.rom_info = rom_info;
    }

    pub fn setup_saveram(&mut self, name: &str) {
        self.save_ram = if let Ok(Some(encoded)) = self.storage.get_item(name) {
            let save_ram = STANDARD.decode(encoded).unwrap_or_default();
            Arc::new(Mutex::new(save_ram))
        } else {
            Arc::new(Mutex::new(Vec::new()))
        };
    }

    pub fn save_current(&mut self, name: &str) {
        const SAVE_INTERVAL: u64 = 5;
        if self.last_save.elapsed() > Duration::from_secs(SAVE_INTERVAL) {
            if let Some(rom_info) = &self.rom_info {
                if !rom_info.is_battery_backed() {
                    return;
                }
            }

            if let Ok(save_ram) = &self.save_ram.try_lock() {
                let encoded = STANDARD.encode(save_ram.to_vec());
                self.storage.set_item(name, &encoded).unwrap();
            }
            self.last_save = Instant::now();
        }
    }

    pub fn save(&mut self, name: &str, data: &[u8]) {
        let encoded = STANDARD.encode(data);
        self.storage.set_item(name, &encoded).unwrap();
    }

    pub fn load_bootrom(
        &mut self,
        rom_type: &CartType,
        bootrom_options: &BootRomOptions,
    ) -> Option<Vec<u8>> {
        let mut boot_rom = match (&bootrom_options.gb_type, &rom_type) {
            (None, CartType::GB)
            | (Some(GameboyType::DMG), CartType::GB)
            | (Some(GameboyType::DMG), CartType::Hybrid)
            | (Some(GameboyType::DMG), CartType::CGB) => self.load(DMG_ROM_NAME),
            (None, CartType::CGB)
            | (None, CartType::Hybrid)
            | (Some(GameboyType::CGB), CartType::GB)
            | (Some(GameboyType::CGB), CartType::CGB)
            | (Some(GameboyType::CGB), CartType::Hybrid) => self.load(CGB_ROM_NAME),
        };

        if !bootrom_options.use_bootrom {
            boot_rom = None;
        }

        boot_rom
    }

    fn load(&mut self, name: &str) -> Option<Vec<u8>> {
        let encoded = self
            .storage
            .get_item(name)
            .unwrap_or(None)
            .unwrap_or_default();
        STANDARD.decode(encoded).ok()
    }

    pub fn download_all(&mut self) -> Result<(), String> {
        let cursor = io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o755)
            .last_modified_time(zip::DateTime::default_for_write());

        for i in 0..=self.storage.length().unwrap_or(0) {
            let Ok(Some(key)) = self.storage.key(i) else {
                log::error!("Unable to get key at storage index: {i}");
                continue;
            };

            if let Ok(Some(item)) = self.storage.get(&key) {
                let item = item.replace("\"", "");
                match &STANDARD.decode(item) {
                    Ok(decoded) => {
                        zip.start_file(format!("{key}.sav").into_boxed_str(), options)
                            .unwrap_or(());
                        zip.write_all(decoded).unwrap_or_default();
                    }
                    Err(err) => log::error!("{err}"),
                }
            }
        }

        let encoded = match zip.finish() {
            Ok(cursor) => STANDARD.encode(cursor.into_inner()),
            Err(err) => {
                return Err(format!("{err}"));
            }
        };

        Saves::download_helper("saves.zip", &encoded)?;

        Ok(())
    }

    fn download_helper(name: &str, base64_data: &str) -> Result<(), String> {
        if let Err(err) = STANDARD.decode(base64_data) {
            return Err(format!("String is not base64: {err}"));
        }

        let win = web_sys::window().ok_or("unknown error".to_string())?;
        let doc = win.document().ok_or("unknown error".to_string())?;

        let link = doc.create_element("a").unwrap();
        link.set_attribute(
            "href",
            &format!("data:application/octet-stream;base64,{base64_data}"),
        )
        .map_err(|e| e.as_string().unwrap_or("unknown error".to_string()))?;
        link.set_attribute("download", name)
            .map_err(|e| e.as_string().unwrap_or("unknown error".to_string()))?;

        let link = web_sys::HtmlAnchorElement::unchecked_from_js(link.into()); // Figure out a better way to do this
        link.click();

        Ok(())
    }

    pub fn upload(&mut self) {
        crate::app::open(
            &self.events,
            &[("Gameboy Save Ram File", &["sav"]), ("All Files", &["*"])],
            crate::app::EventType::SaveUpload,
        );
    }

    pub fn show_save_manager(&mut self, ui: &mut egui::Ui) {
        let excluded: [String; 4] = [
            "app".into(),
            "egui_memory_ron".into(),
            crate::app::DMG_ROM_NAME.into(),
            crate::app::CGB_ROM_NAME.into(),
        ];
        if self.save_data.is_empty() {
            for i in 0..=self.storage.length().unwrap_or(0) {
                let Ok(Some(key)) = self.storage.key(i) else {
                    log::error!("Unable to get key at storage index: {i}");
                    continue;
                };
                if let Ok(Some(item)) = self.storage.get(&key) {
                    if !excluded.contains(&key) {
                        // Ignore egui/app entries
                        self.save_data.insert(key.clone(), (key, item));
                    }
                };
            }
        }

        egui::Grid::new("save_manager")
            .min_col_width(0.0)
            .show(ui, |ui| {
                let mut modified: bool = false;
                for (key, (key_field, item)) in &mut self.save_data {
                    ui.horizontal(|ui| {
                        ui.set_width(200.0);
                        if ui.text_edit_singleline(key_field).lost_focus() && key != key_field {
                            let _ = self.storage.set(key_field, item);
                            let _ = self.storage.delete(key);
                            modified = true;
                        };
                    });

                    if ui.button("⬇").clicked() {
                        let _ = Saves::download_helper(&format!("{key_field}.sav"), item);
                        ui.close_menu();
                    }

                    if ui.button("X").clicked() {
                        let _ = self.storage.delete(key);
                        modified = true;
                    };
                    ui.end_row();
                }
                if modified {
                    self.save_data.clear();
                }
            });

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            if ui.button("Upload").clicked() {
                self.upload();
            }
            if ui.button("Download All").clicked() {
                if let Err(err) = self.download_all() {
                    log::error!("{err}")
                }
            }
        });
    }
}
