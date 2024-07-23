
use std::sync::{atomic::AtomicU8, Arc};

use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, FromSample, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
};
use solgb::gameboy::AudioControl;

pub struct Audio {
    pub device: Device,
    pub config: SupportedStreamConfig,
    pub volume: Arc<AtomicU8>,
}

impl Audio {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        log::info!("Output device: {}", device.name().unwrap());
        let config = device.default_output_config().unwrap();
        log::info!("Default output config: {:?}", config);

        let volume = Arc::new(AtomicU8::new(0));

        Self { device, config, volume }
    }

    pub fn get_stream(&self, sample_rec: AudioControl) -> Stream {
        match self.config.sample_format() {
            cpal::SampleFormat::I8 => self.setup::<i8>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::I16 => self.setup::<i16>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::I32 => self.setup::<i32>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::I64 => self.setup::<i64>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::U8 => self.setup::<u8>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::U16 => self.setup::<u16>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::U32 => self.setup::<u32>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::U64 => self.setup::<u64>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::F32 => self.setup::<f32>(sample_rec, self.volume.clone()),
            cpal::SampleFormat::F64 => self.setup::<f64>(sample_rec, self.volume.clone()),
            sample_format => panic!("Unsupported sample format '{sample_format}'"),
        }
    }

    fn setup<T>(&self, mut sample_rec: AudioControl, volume: Arc<AtomicU8>) -> Stream
    where
        T: SizedSample + FromSample<f32>,
    {
        let config: StreamConfig = self.config.clone().into();
        log::info!("Actual output config: {:?}", config);
        let mut last = 0f32;

        match config.channels {
            2 => {
                self.device.build_output_stream(
                    &config,
                    {
                        let mut buffer = Vec::new().into_iter();
                        move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                            for value in out.iter_mut() {
                                last = match buffer.next() {
                                    Some(val) => val,
                                    None => {
                                        loop { //This jank is because we cant block
                                            if let Ok(samples) = sample_rec.try_get_audio_buffer() {
                                                buffer = samples.into_iter();
                                                break
                                            }
                                        }
                                        buffer.next().unwrap_or(last)
                                    }
                                };
                                let volume = (volume.load(std::sync::atomic::Ordering::Relaxed) as f32) / 100.0;
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
            _ => panic!("Unable to create audio stream: Unsupported number of channel"),
        }
        .unwrap()
    }
}
