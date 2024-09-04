use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, FromSample, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
};
use crossbeam_channel::{Receiver, Sender};
use solgb::AudioControl;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

pub struct Audio {
    pub device: Device,
    pub config: SupportedStreamConfig,
    stream: Option<Stream>,
    volume: Arc<AtomicU8>,
    ac_receiver: Receiver<AudioControl>,
    ac_sender: Sender<AudioControl>,
    audio_control: Option<AudioControl>,
}

impl Audio {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        log::info!("Output device: {}", device.name().unwrap());
        let config = device.default_output_config().unwrap();
        log::info!("Default output config: {:?}", config);

        let volume = Arc::new(AtomicU8::new(0));
        let (ac_sender, ac_receiver) = crossbeam_channel::unbounded();

        let mut audio = Self {
            device,
            config,
            stream: None,
            volume,
            ac_receiver,
            ac_sender,
            audio_control: None,
        };
        audio.setup_stream();
        audio
    }

    fn setup_stream(&mut self) {
        self.stream = match self.config.sample_format() {
            cpal::SampleFormat::I8 => self.setup::<i8>(),
            cpal::SampleFormat::I16 => self.setup::<i16>(),
            cpal::SampleFormat::I32 => self.setup::<i32>(),
            cpal::SampleFormat::I64 => self.setup::<i64>(),
            cpal::SampleFormat::U8 => self.setup::<u8>(),
            cpal::SampleFormat::U16 => self.setup::<u16>(),
            cpal::SampleFormat::U32 => self.setup::<u32>(),
            cpal::SampleFormat::U64 => self.setup::<u64>(),
            cpal::SampleFormat::F32 => self.setup::<f32>(),
            cpal::SampleFormat::F64 => self.setup::<f64>(),
            sample_format => panic!("Unsupported sample format '{sample_format}'"),
        };
    }

    pub fn play(&mut self) {
        self.setup_stream();

        let Some(stream) = &self.stream else {
            log::error!("Failed to play stream: Stream is not setup (bad device/config?)");
            return;
        };

        if let Err(err) = stream.play() {
            log::error!("Failed to play stream: {err}");
        }
    }

    pub fn pause(&self) {
        let Some(stream) = &self.stream else {
            log::error!("Failed to pause stream: Stream is not setup (bad device/config?)");
            return;
        };

        if let Err(err) = stream.pause() {
            log::error!("Failed to pause stream: {err}");
        }
    }

    pub fn set_audio_control(&mut self, audio_control: AudioControl) {
        self.audio_control = Some(audio_control.clone());
        if let Err(err) = self.ac_sender.send(audio_control) {
            log::error!("Unable to send AudioControl to callback: {err}");
        }
    }

    pub fn set_volume(&self, mut volume: u8) {
        if volume > 100 {
            volume = 100;
        }
        self.volume.store(volume, Ordering::Relaxed)
    }

    fn setup<T>(&mut self) -> Option<Stream>
    where
        T: SizedSample + FromSample<f32>,
    {
        const TIMEOUT: Duration = Duration::from_millis(20);

        let config: StreamConfig = self.config.clone().into();
        log::info!("Actual output config: {:?}", config);
        let mut last = 0f32;
        let volume = self.volume.clone();
        let ac_receiver = self.ac_receiver.clone();
        let mut audio_control = self.audio_control.clone();

        match config.channels {
            2 => {
                self.device.build_output_stream(
                    &config,
                    {
                        let mut buffer = Vec::new().into_iter();
                        move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                            if let Ok(ac) = ac_receiver.try_recv() {
                                log::info!("Loaded new AudioControl");
                                audio_control = Some(ac);
                            }

                            let Some(sample_rec) = &audio_control else {
                                out.fill(T::from_sample(0.0));
                                return;
                            };

                            for value in out.iter_mut() {
                                last = match buffer.next() {
                                    Some(val) => val,
                                    None => {
                                        let start = Instant::now();
                                        loop {
                                            //This jank is because we can't block
                                            if let Ok(samples) = sample_rec.try_get_audio_buffer() {
                                                buffer = samples.into_iter();
                                                break;
                                            }
                                            if Instant::now().duration_since(start) > TIMEOUT {
                                                return;
                                            }
                                        }
                                        buffer.next().unwrap_or(last)
                                    }
                                };
                                let volume = (volume.load(Ordering::Relaxed) as f32) / 100.0;
                                *value = T::from_sample(last * volume);
                            }
                        }
                    },
                    move |err| {
                        log::error!("Audio callback error: {}", err);
                    },
                    None,
                )
            }
            _ => panic!(),
        }
        .ok()
    }
}
