mod encoder;

use std::{path::Path, sync::mpsc, time::Duration};

use anyhow::Error;
use bytes::Bytes;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    StreamInstant,
};
use encoder::{Encoder, VideoEncoder};

const OUTPUT_FILE: &str = "./audio.mp3";

fn main() -> Result<(), anyhow::Error> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or(Error::msg("Invalid device"))?;

    let config = device.default_input_config()?;
    let foramt = config.sample_format();

    println!("Config: {:?}", config);

    let output = Path::new(OUTPUT_FILE);
    let mut encoder = VideoEncoder::init(output)?;

    println!("encoder initialized");

    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        println!("encoder thread started");
        while let Ok(Some(audio_sample)) = rx.recv() {
            println!("got cpal sample");
            encoder
                .append_audio(audio_sample)
                .expect("couldn't encode frame");
        }
        encoder.finish().expect("couldn't finish encoding");
    });

    let mut t: Option<StreamInstant> = None;

    let tx_clone = tx.clone();

    let stream = device
        .build_input_stream_raw(
            &config.config(),
            foramt,
            move |raw_data, info| {
                let pts = {
                    let capture = info.timestamp().capture;
                    capture.duration_since(&t.get_or_insert(capture)).unwrap()
                };
                let data = Bytes::copy_from_slice(raw_data.bytes());
                tx_clone
                    .send(Some(AudioSample { data, pts }))
                    .expect("couldn't send frame");
            },
            |e| println!("Error: {:?}", e),
            None,
        )
        .expect("Failed to build input stream");

    stream.play().expect("Failed to play stream");

    std::thread::sleep(std::time::Duration::from_secs(2));

    stream.pause().expect("Failed to pause stream");

    tx.send(None).expect("couldn't send no-op");

    handle.join().expect("couldn't complete encoding thread");

    Ok(())
}

pub struct AudioSample {
    data: Bytes,
    pts: Duration,
}
