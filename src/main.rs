
mod encoder;

use anyhow::Error;
use core_graphics::display::{CGPoint, CGRect, CGSize};
use std::sync::mpsc;
use std::path::Path;
use encoder::Encoder;

use core_foundation::error::CFError;
use core_media_rs::{cm_sample_buffer::CMSampleBuffer, cm_time::CMTime};
use screencapturekit::{
    shareable_content::SCShareableContent, stream::{
        configuration::SCStreamConfiguration, content_filter::SCContentFilter,
        output_trait::SCStreamOutputTrait, output_type::SCStreamOutputType, SCStream,
    }
};

use std::sync::mpsc::Sender;

struct VideoStreamOutput {
    sender: Sender<CMSampleBuffer>,
}

impl SCStreamOutputTrait for VideoStreamOutput {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        _of_type: SCStreamOutputType,
    ) {
        self.sender
            .send(sample_buffer)
            .expect("could not send to output_buffer");
    }
}

const OUTPUT_FILE: &str = "./video.mp4";

fn main() -> Result<(), Error> {

    let scale = 2;
    // let height =  1080.0;
    // let width = 2560.0;

    let height =  900.0;
    let width = 1440.0;

    let output = Path::new(OUTPUT_FILE);
    let mut encoder = encoder::AVAssetWriterEncoder::init(height, width, output)?;

    let (tx, rx) = mpsc::channel();
    let stream = get_stream(tx, height, width, scale).unwrap();
    stream.start_capture().unwrap();

        let max_number_of_samples: i32 = 100;


        for sample_index in 0..max_number_of_samples {
        let sample_buf = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("could not receive from output_buffer");
            encoder.append_frame(sample_buf, sample_index).expect("couldn't encode frame");
        }

        encoder.finish().expect("couldn't finish encoding");


    std::thread::sleep(std::time::Duration::from_secs(3));
    stream.stop_capture().unwrap();

    println!("finished!");

    Ok(())
}

fn get_stream(tx: Sender<CMSampleBuffer>, height: f64, width: f64, scale : u32) -> Result<SCStream, CFError> {
    let wu = width as u32;
    let hu = height as u32;
    let config = SCStreamConfiguration::new().set_captures_audio(false)?.set_source_rect(CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { width , height} })?
        .set_width(wu * 2)?
        .set_height(hu * 2)?
    .set_pixel_format(screencapturekit::stream::configuration::pixel_format::PixelFormat::BGRA)?
    .set_minimum_frame_interval(&CMTime{value: 1, timescale: 120, ..Default::default()})?
    ;

    let display = SCShareableContent::get().unwrap().displays().remove(0);
    println!("display={:?}", display);
    let filter = SCContentFilter::new().with_display_excluding_windows(&display, &[]);
    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(VideoStreamOutput { sender: tx }, SCStreamOutputType::Screen);

    Ok(stream)
}

