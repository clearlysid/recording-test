use anyhow::Error;
use crabgrab::prelude::WindowsDx11VideoFrame;
use std::path::Path;
use std::time::Instant;

use crate::Encoder;

mod encoder;

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

            // Set up timestamp
            let ts = video_frame.capture_time();
            if self.first_ts.is_none() {
                self.first_ts = Some(ts)
            }

            let (dx11_surface, pixel_format) = video_frame.get_dx11_surface()?;
            let (dx11_texture, _) = video_frame.get_dx11_texture()?;

            
            // Convert the CrabGrab frame to WCFrame


            // let frame = Frame::new(d3d_device, dx11_surface, dx11_texture, time, context, buffer, width, height, color_format);

            //
            // encoder.send_frame(frame)

            todo!("still working");
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