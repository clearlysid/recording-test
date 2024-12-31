use anyhow::Error;
use arc::Retained;
use av::sample_buffer;
use cidre::{objc::Obj, *};
use core_media_rs::cm_sample_buffer::CMSampleBuffer;
use screencapturekit::{output::sc_stream_frame_info::SCStreamFrameInfo, utils::objc::MessageForTFType};
use std::path::Path;

use super::Encoder;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVVideoAverageBitRateKey: &'static cidre::ns::String;
    static AVVideoProfileLevelKey: &'static cidre::ns::String;
    static AVVideoProfileLevelH264HighAutoLevel: &'static cidre::ns::String;

    static AVVideoTransferFunctionKey: &'static cidre::ns::String;
    static AVVideoTransferFunction_ITU_R_709_2: &'static cidre::ns::String;
    static AVVideoColorPrimariesKey: &'static cidre::ns::String;
    static AVVideoColorPrimaries_ITU_R_709_2: &'static cidre::ns::String;
    static AVVideoYCbCrMatrixKey: &'static cidre::ns::String;
    static AVVideoYCbCrMatrix_ITU_R_709_2: &'static cidre::ns::String;
}

pub struct AVAssetWriterEncoder {
    writer: Retained<av::AssetWriter>,
    input: Retained<av::AssetWriterInput>,
    first_ts: Option<cm::Time>,
    last_ts: Option<cm::Time>,
}

impl AVAssetWriterEncoder {
    pub fn init(height: f64, width: f64, output: &Path) -> Result<Self, Error> {
        let mut writer = av::AssetWriter::with_url_and_file_type(
            cf::Url::with_path(output, false).unwrap().as_ns(),
            av::FileType::mp4(),
        )?;

        let assistant =
            av::OutputSettingsAssistant::with_preset(av::OutputSettingsPreset::h264_3840x2160())
                .ok_or(Error::msg("Failed to create output settings assistant"))?;

        let mut dict = assistant
            .video_settings()
            .ok_or(Error::msg("No assistant video settings"))?
            .copy_mut();

        dict.insert(
            av::video_settings_keys::width(),
            ns::Number::with_u32(width as u32).as_id_ref(),
        );

        dict.insert(
            av::video_settings_keys::height(),
            ns::Number::with_u32(height as u32).as_id_ref(),
        );

        let mut compression_flags = ns::DictionaryMut::new();

        compression_flags.insert(
            unsafe { AVVideoProfileLevelKey },
            unsafe { AVVideoProfileLevelH264HighAutoLevel }.as_id_ref(),
        );
        compression_flags.insert(
            unsafe { AVVideoAverageBitRateKey },
            ns::Number::with_u32(10_000_000).as_id_ref(),
        );

        dict.insert(
            av::video_settings_keys::compression_props(),
            compression_flags.as_id_ref(),
        );

        let mut color_flags = ns::DictionaryMut::new();

        color_flags.insert(unsafe { AVVideoTransferFunctionKey }, unsafe {
            AVVideoTransferFunction_ITU_R_709_2
        });
        color_flags.insert(unsafe { AVVideoColorPrimariesKey }, unsafe {
            AVVideoColorPrimaries_ITU_R_709_2
        });
        color_flags.insert(unsafe { AVVideoYCbCrMatrixKey }, unsafe {
            AVVideoYCbCrMatrix_ITU_R_709_2
        });

        dict.insert(
            av::video_settings_keys::color_props(),
            color_flags.as_id_ref(),
        );

        let mut input = av::AssetWriterInput::with_media_type_and_output_settings(
            av::MediaType::video(),
            Some(dict.as_ref()),
        )
        .map_err(|_| Error::msg("Failed to create AVAssetWriterInput"))?;
        input.set_expects_media_data_in_real_time(true);

        writer
            .add_input(&input)
            .map_err(|_| Error::msg("Failed to add asset writer input"))?;

        writer.start_writing();

        Ok(Self {
            input,
            writer,
            first_ts: None,
            last_ts: None,
        })
    }
}

impl Encoder for AVAssetWriterEncoder {
    fn append_frame(&mut self, sample_buffer: CMSampleBuffer, sample_index: i32) -> Result<(), Error> {
        if !self.input.is_ready_for_more_media_data() {
            println!("not ready for more data");
            return Ok(());
        }
        let frameinfo = SCStreamFrameInfo::from_sample_buffer(&sample_buffer);
        println!("frameinfo={:?}", frameinfo);
        let sample_buf = sample_buffer.as_sendable();

        // println!("cms={:?}", sample_buf);

        let sample_buf = unsafe { &*(sample_buf as *mut cm::SampleBuf) };

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
