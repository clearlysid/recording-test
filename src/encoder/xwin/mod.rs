use anyhow::Error;
use crabgrab::prelude::WindowsDx11VideoFrame;
use std::path::Path;
use std::time::Instant;

use crate::Encoder;
mod encoder;

use windows::Foundation::TimeSpan;
use encoder::{AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder};


pub struct WindowsCaptureEncoder {
    encoder: Option<VideoEncoder>,
    first_ts: Option<Instant>,
    last_ts: Option<Instant>
}

impl WindowsCaptureEncoder {
    pub fn init(height: f64, width: f64, output: &Path) -> Result<Self, Error> {

        let video_settings = VideoSettingsBuilder::new(width as u32, height as u32);
        let container_settings = ContainerSettingsBuilder::default();
        let audio_settings = AudioSettingsBuilder::default().disabled(true);

        let encoder = VideoEncoder::new(video_settings, audio_settings, container_settings, output)?;

        Ok(Self {
            encoder: Some(encoder),
            first_ts: None,
            last_ts: None
        })
    }
}



impl Encoder for WindowsCaptureEncoder {
    fn append_frame(&mut self, video_frame: crabgrab::prelude::VideoFrame) -> Result<(), anyhow::Error> {
        if let Some(encoder) = self.encoder.as_mut() {

            // Process timestamp
            let ts = video_frame.capture_time();
            if self.first_ts.is_none() {
                self.first_ts = Some(ts)
            }

            let ts_delta = ts.duration_since(self.first_ts.unwrap());
            let ts_delta_nanos = ts_delta.as_nanos() as i64;

            let timespan = TimeSpan { Duration: ts_delta_nanos };

            // Get IDirect3DSurface
            let (dx11_surface, _) = video_frame.get_dx11_surface()?;

            // Send surface to encoder
            encoder.send_surface(dx11_surface, timespan)?;
            
            Ok(())
        } else {
            Err(Error::msg("no active encoder"))
        }
    }

    fn finish(&mut self) -> Result<(), anyhow::Error> {
        if let Some(enc) = self.encoder.take() {
            enc.finish()?;
            Ok(())
        } else {
            Err(Error::msg("no encoder active"))
        }
    }
}