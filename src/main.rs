mod encoder;

use std::{path::Path, sync::mpsc, time::Instant};

use anyhow::Error;
use bytes::Bytes;
use encoder::{Encoder, VideoEncoder};
use nokhwa::{
    native_api_backend,
    pixel_format::RgbAFormat,
    query,
    utils::{CameraFormat, FrameFormat, RequestedFormat, RequestedFormatType},
    CallbackCamera,
};

const OUTPUT_FILE: &str = "./video.mp4";

fn main() -> Result<(), Error> {
    let backend = native_api_backend().unwrap();
    let camera_list = query(backend)?;

    let cam_info = camera_list.first().unwrap();

    let cam_index = cam_info.index().to_owned();

    let format = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestResolution);

    let mut camera = CallbackCamera::new(cam_index, format, |_| {})?;

    println!("camera rez: {}", camera.resolution().unwrap());

    let width = camera.resolution().unwrap().width();
    let height = camera.resolution().unwrap().height();

    let output = Path::new(OUTPUT_FILE);
    let mut encoder = VideoEncoder::init(height, width, output)?;

    // TOFIX: my code doesn't run past this point and this log never gets printed.
    println!("encoder created");

    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        while let Ok(Some(frame)) = rx.recv() {
            encoder.append_frame(frame).expect("couldn't encode frame");
        }

        encoder.finish().expect("couldn't finish encoding");
    });

    let tx_clone = tx.clone();
    camera.set_callback(move |buffer| {
        let ts = std::time::Instant::now();

        let width = buffer.resolution().width();
        let height = buffer.resolution().height();
        println!("{}", buffer.source_frame_format());

        let data = buffer.buffer_bytes();

        let frame = VideoFrame {
            width,
            height,
            data,
            ts,
        };

        tx_clone.send(Some(frame)).expect("couldn't send frame");
    })?;
    camera.open_stream()?;

    std::thread::sleep(std::time::Duration::from_secs(2));

    camera.set_callback(move |_| {
        tx.send(None).expect("couldn't send end-of-stream signal");
    })?;

    handle.join().expect("couldn't join handle");
    Ok(())
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Bytes,
    pub ts: Instant,
}
