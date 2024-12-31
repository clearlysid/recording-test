use std::path::Path;
use arc::Retained;
use cidre::{objc::Obj, *};
use anyhow::Error;

use super::Encoder;

pub struct AVAssetWriterEncoder {
    writer: Retained<av::AssetWriter>,
    input: Retained<av::AssetWriterInput>,
    first_ts: Option<cm::Time>,
    last_ts: Option<cm::Time>,
}

impl AVAssetWriterEncoder {
    pub fn init(height: f64, width: f64, output: &Path) -> Result<Self, Error> {
        let mut asset_writer = av::AssetWriter::with_url_and_file_type(
            cf::Url::with_path(output, false)
                .unwrap()
                .as_ns(),
            av::FileType::mp4(),
        )?;

        let assistant =
            av::OutputSettingsAssistant::with_preset(av::OutputSettingsPreset::h264_3840x2160())
                .ok_or(Error::msg("Failed to create output settings assistant"))?;

        let mut dict = assistant.video_settings()
            .ok_or(Error::msg("No assistant video settings"))?.copy_mut();

        dict.insert(
            av::video_settings_keys::width(),
            ns::Number::with_u32(width as u32).as_id_ref(),
        );

        dict.insert(
            av::video_settings_keys::height(),
            ns::Number::with_u32(height as u32).as_id_ref(),
        );

        dict.insert(
            av::video_settings_keys::compression_props(),
            ns::Dictionary::with_keys_values(
                &[unsafe { AVVideoAverageBitRateKey }],
                &[ns::Number::with_u32(10_000_000).as_id_ref()],
            )
            .as_id_ref(),
        );

        let mut video_input = av::AssetWriterInput::with_media_type_and_output_settings(
            av::MediaType::video(),
            Some(dict.as_ref()),
        )
        .map_err(|_| Error::msg("Failed to create AVAssetWriterInput"))?;
        video_input.set_expects_media_data_in_real_time(true);

        asset_writer
            .add_input(&video_input)
            .map_err(|_| Error::msg("Failed to add asset writer input"))?;

        asset_writer.start_writing();

        Ok(Self {
            input: video_input,
            writer: asset_writer,
            first_ts: None,
            last_ts: None,
        })
    }
}

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVVideoAverageBitRateKey: &'static cidre::ns::String;
}

impl Encoder for AVAssetWriterEncoder {
    fn append_frame(&mut self, frame: crabgrab::prelude::VideoFrame) -> Result<(), Error> {
        if !self.input.is_ready_for_more_media_data() {
            println!("not ready for more data");
            return Ok(())
        }

        // Get CMSampleBuffer from capturer and do some type gymnastics to cast it
        let sample_buf = frame.get_cm_sample_buffer();
        let sample_buf = unsafe {
            let ptr = &*sample_buf as *const _ as *const cm::SampleBuf;
            &*ptr
        };
            
        let time = sample_buf.pts();

        if self.first_ts.is_none() {
            self.writer.start_session_at_src_time(time);
            self.first_ts = Some(time);
        }

        self.last_ts = Some(time);

        self.input.append_sample_buf(sample_buf).ok();

        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        self.writer
            .end_session_at_src_time(self.last_ts.take().unwrap_or(cm::Time::zero()));
        self.input.mark_as_finished();
        self.writer.finish_writing();

        Ok(())
    }
}