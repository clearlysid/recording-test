use std::fs::File;
use std::time::Instant;
use super::Encoder;
use anyhow::Error;

use ac_ffmpeg::format::muxer::{Muxer, OutputFormat};
use ac_ffmpeg::codec::Encoder as ACEncoder;
use ac_ffmpeg::codec::video::frame::get_pixel_format;
use ac_ffmpeg::codec::video::{VideoEncoder, VideoFrame, VideoFrameMut, VideoFrameScaler};
use ac_ffmpeg::time::{TimeBase, Timestamp};
use ac_ffmpeg::format::io::IO;

use crabgrab::feature::bitmap::{FrameBitmapBgraUnorm8x4, FrameBitmapYCbCr};
use crabgrab::prelude::VideoFrameBitmap;
use crabgrab::prelude::FrameBitmap::{BgraUnorm8x4, YCbCr};


pub struct EncoderAcFfmpeg {
    muxer: Muxer<File>,
    first_ts: Option<Instant>,
    encoder: VideoEncoder,
}

impl EncoderAcFfmpeg {
    pub fn init(height: f64, width: f64, path: &std::path::Path) -> Result<Self, Error> {
        let pf = get_pixel_format("yuv420p");
        let codec = "libx264";
        let time_base = TimeBase::MICROSECONDS;

        let encoder_builder = VideoEncoder::builder(codec)?
            // Ensure proper bitrate (in bits per second)
            .bit_rate(2_000_000)  // 2Mbps
            .pixel_format(pf)
            .time_base(time_base)
            .width(width as usize)
            .height(height as usize)
            .set_option("fps", "60")
            // Basic quality settings
            .set_option("preset", "fast")  // Balance between speed and quality
            .set_option("tune", "zerolatency")  // Better for screen recording
            .set_option("crf", "13")  // Default CRF value, lower means better quality
            .set_option("color_range", "jpeg")
            // .set_option("colormatrix", "bt709")
            // .set_option("colorprim", "bt709")
            // .set_option("transfer", "bt709")
            ;

        let encoder = encoder_builder.build()?;
        let cp = encoder.codec_parameters();

        let file = File::create_new(path)?;
        let io = IO::from_seekable_write_stream(file);

        let output_format = OutputFormat::guess_from_file_name(path.file_name().unwrap().to_str().unwrap())
            .ok_or(Error::msg("Failed to guess output format"))?;

        let mut muxer_builder = Muxer::builder();
        muxer_builder.add_stream(&cp.into())?;

        let muxer = muxer_builder.build(io, output_format)?;


        Ok(EncoderAcFfmpeg { first_ts: None, encoder, muxer })
    }
}

impl Encoder for EncoderAcFfmpeg {
    fn append_frame(&mut self, frame: crabgrab::prelude::VideoFrame) -> Result<(), Error> {
        // Update first_ts if it no exist
        let ts = frame.capture_time();
        if self.first_ts.is_none() {
            self.first_ts = Some(ts)
        }

        // Create ts
        let pts_raw = ts.duration_since(self.first_ts.unwrap()).as_micros();
        let pts = Timestamp::from_micros(pts_raw as i64);

        let frame = create_acff_videoframe_from_crabgrab_frame(frame)?;

        let cp = self.encoder.codec_parameters();
        let target_pf = cp.pixel_format();
        let target_height = cp.height();
        let target_width = cp.width();

        // Resize/convert frame to compatible one
        let scaled_frame = VideoFrameScaler::builder()
            .source_height(frame.height())
            .source_width(frame.width())
            .source_pixel_format(frame.pixel_format())
            .target_height(target_height)
            .target_width(target_width)
            .target_pixel_format(target_pf)
            .build()?
            .scale(&frame)?;

        self.encoder.push(scaled_frame.with_pts(pts))?;

        while let Ok(Some(p)) = self.encoder.take() {
            self.muxer.push(p)?;
        }

        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        self.encoder.flush()?;
        self.muxer.flush()?;

        Ok(())
    }
}


fn create_acff_videoframe_from_crabgrab_frame(source: crabgrab::prelude::VideoFrame) -> Result<VideoFrame, Error> {
    let width = source.size().width as usize;
    let height = source.size().height as usize;

    let bitmap = source.get_bitmap()?;
    let pf = match bitmap {
        BgraUnorm8x4(_) => get_pixel_format("bgra"),
        YCbCr(_) => get_pixel_format("nv12"),
        _ => todo!("what format you give bro?")
    };

    let mut black_frame = VideoFrameMut::black(pf, width, height);

    match bitmap {
        BgraUnorm8x4(FrameBitmapBgraUnorm8x4 { data, width, .. }) => {
            let bytes = data.as_flattened();
            let stride = black_frame.planes()[0].line_size();

            for (out_line, in_line) in black_frame.planes_mut()[0]
                .data_mut()
                .chunks_mut(stride)
                .zip(bytes.chunks(width * 4))
            {
                out_line[..width * 4].copy_from_slice(in_line);
            }
        }
        YCbCr(FrameBitmapYCbCr { luma_data, chroma_data, .. }) => {
            let y_bytes = luma_data.as_ref();
            let uv_bytes = chroma_data.as_flattened();

            let y_stride = black_frame.planes()[0].line_size();

            for (out_line, in_line) in black_frame.planes_mut()[0]
                .data_mut()
                .chunks_mut(y_stride)
                .zip(y_bytes.chunks(width))
            {
                out_line[..width].copy_from_slice(in_line);
            }

            let uv_stride = black_frame.planes()[1].line_size();

            for (out_line, in_line) in black_frame.planes_mut()[1]
                .data_mut()
                .chunks_mut(uv_stride)
                .zip(uv_bytes.chunks(width))
            {
                out_line[..width].copy_from_slice(in_line);
            }
        }
        _ => unimplemented!("what format you give bro?")
    }


    let frame = black_frame.freeze();

    Ok(frame)
}
