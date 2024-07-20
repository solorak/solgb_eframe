
use std::{io::{self, Write}, sync::{Arc, Mutex}};
use web_sys::Storage;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use web_time::{Duration, Instant};
use web_sys;
use wasm_bindgen::JsCast;
use zip::write::SimpleFileOptions;

pub struct Saves {
    pub storage: Storage,
    last_save: Instant,
    pub save_ram: Arc<Mutex<Vec<u8>>>,
}

impl Saves {
    pub fn new() -> Option<Self> {
        let Some(Some(storage)) = web_sys::window().and_then(|s| s.local_storage().ok()) else {
            return None
        };
        Some(Self {
            storage,
            last_save: Instant::now(),
            save_ram: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn save_current(&mut self, current_name: &str) {
        const SAVE_INTERVAL: u64 = 5;
        if self.last_save.elapsed() > Duration::from_secs(SAVE_INTERVAL) {
            log::info!("Attempting to try_lock save ram mutex");
            if let Ok(save_ram) = &self.save_ram.try_lock() {
                let encoded = STANDARD.encode(save_ram.to_vec());
                self.storage.set_item(current_name, &encoded).unwrap();
                self.last_save = Instant::now();
                log::info!("Stored save ram data for {current_name}");
            }
            log::info!("done with save ram mutex");
        }
    }

    pub fn download(&mut self, name: &str) -> Result<(), String> {
        let item = match self.storage.get(&name) {
            Ok(Some(item)) => item,
            _ => return Err(format!("Unable to retrive item or item with name: {name} does not exist")),
        };

        self.download_helper(name, &item)?;
        Ok(())
    }

    pub fn download_all(&mut self) -> Result<(), String> {
        let cursor = io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o755)
            .last_modified_time(zip::DateTime::default_for_write());

        for i in 0..self.storage.length().unwrap_or(0) {
            let Ok(Some(key)) = self.storage.key(i) else {
                log::error!("Unable to get key at storage index: {i}");
                continue
            };

            if let Ok(Some(item)) = self.storage.get(&key) {
                let item = item.replace("\"", "");
                match &STANDARD.decode(item) {
                    Ok(decoded) => {
                        zip.start_file(key, options).unwrap_or(());
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

        self.download_helper("saves.zip", &encoded)?;

        Ok(())
    }

    fn download_helper(&mut self, name: &str, base64_data: &str) -> Result<(), String> {

        if let Err(err) = STANDARD.decode(base64_data) {
            return Err(format!("String is not base64: {err}"));
        }

        let win = web_sys::window().ok_or(format!("unknown error"))?;
        let doc = win.document().ok_or(format!("unknown error"))?;

        let link = doc.create_element("a").unwrap();
        link.set_attribute("href", &format!("data:application/octet-stream;base64,{base64_data}")).map_err(|e| e.as_string().unwrap_or(format!("unknown error")))?;
        link.set_attribute("download", name).map_err(|e| e.as_string().unwrap_or(format!("unknown error")))?;

        let link = web_sys::HtmlAnchorElement::unchecked_from_js(link.into()); // Figure out a better way to do this
        link.click();

        Ok(())
    }
}