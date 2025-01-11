use anyhow::Error;
use windows::Security::Cryptography::CryptographicBuffer;

use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use crate::encoder::Encoder;

use windows::core::HSTRING;
use windows::Foundation::{EventRegistrationToken, TimeSpan, TypedEventHandler};
use windows::Media::Core::{
    AudioStreamDescriptor, MediaStreamSample, MediaStreamSource,
    MediaStreamSourceSampleRequestedEventArgs, MediaStreamSourceStartingEventArgs,
};
use windows::Media::MediaProperties::{
    AudioEncodingProperties, ContainerEncodingProperties, MediaEncodingProfile,
};
use windows::Media::Transcoding::MediaTranscoder;
use windows::Storage::{FileAccessMode, StorageFile};

pub struct WmfEncoder {
    starting: EventRegistrationToken,
    sample_tx: Sender<Option<MediaStreamSample>>,
    sample_requested: EventRegistrationToken,
    media_stream_source: MediaStreamSource,
    transcode_thread: Option<JoinHandle<Result<(), Error>>>,
}

impl WmfEncoder {
    pub fn init(output: &Path) -> Result<Self, Error> {
        let a_props_output = AudioEncodingProperties::new()?;
        a_props_output.SetSubtype(&HSTRING::from("MP3"))?;
        a_props_output.SetBitrate(192_000)?;
        a_props_output.SetChannelCount(2)?;
        a_props_output.SetBitsPerSample(32)?;
        a_props_output.SetSampleRate(48_000)?;

        // Setup container properties (MPEG4 in our case)
        let container_props = ContainerEncodingProperties::new()?;
        container_props.SetSubtype(&HSTRING::from("MP3"))?;

        // Create a media profile from the above properties
        let media_profile = MediaEncodingProfile::new()?;
        media_profile.SetAudio(&a_props_output)?;
        media_profile.SetContainer(&container_props)?;

        let a_props_source = AudioEncodingProperties::new()?;
        a_props_source.SetSubtype(&HSTRING::from("FLOAT"))?;
        a_props_source.SetSampleRate(48_000)?;
        a_props_source.SetBitsPerSample(32)?;

        let audio_stream_descriptor = AudioStreamDescriptor::Create(&a_props_source)?;

        // Create a media stream source and set the buffer time
        let media_stream_source =
            MediaStreamSource::CreateFromDescriptor(&audio_stream_descriptor)?;
        media_stream_source.SetBufferTime(TimeSpan::default())?;

        let starting = media_stream_source.Starting(&TypedEventHandler::<
            MediaStreamSource,
            MediaStreamSourceStartingEventArgs,
        >::new(move |_, stream_start| {
            let stream_start = stream_start.as_ref().expect("how tf this none?");

            stream_start
                .Request()?
                .SetActualStartPosition(TimeSpan { Duration: 0 })?;
            Ok(())
        }))?;

        println!("media_stream_source started");

        let (sample_tx, sample_rx) = std::sync::mpsc::channel::<Option<MediaStreamSample>>();

        let sample_requested = media_stream_source.SampleRequested(&TypedEventHandler::<
            MediaStreamSource,
            MediaStreamSourceSampleRequestedEventArgs,
        >::new({
            let sample_rx = sample_rx;

            move |_, sample_requested| {
                let sample_requested = sample_requested.as_ref().expect("how tf this none?");

                println!("Sample requested, waiting for sample...");

                let result = sample_rx.recv().unwrap();

                match result {
                    Some(sample) => {
                        println!("Processing sample");
                        sample_requested.Request()?.SetSample(&sample)?;
                    }
                    None => {
                        println!("received end-of-stream signal");
                        sample_requested.Request()?.SetSample(None)?;
                    }
                }

                Ok(())
            }
        }))?;

        println!("media stream sample requested");

        // Set up file for writing into
        std::fs::File::create(&output)?;
        let path = std::fs::canonicalize(&output).unwrap().to_string_lossy()[4..].to_string();
        let path = Path::new(&path);

        let path = &HSTRING::from(path.as_os_str().to_os_string());

        let file = StorageFile::GetFileFromPathAsync(path)?.get()?;
        let media_stream_output = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;

        println!("created the file");

        // Set up MediaTranscoder
        let transcoder = MediaTranscoder::new()?;
        transcoder.SetHardwareAccelerationEnabled(false)?; // disable hardware acceleration for audio

        println!("transcoder created");

        // TOFIX: this part gets stuck if the configs aren't correct
        let transcode = transcoder
            .PrepareMediaStreamSourceTranscodeAsync(
                &media_stream_source,
                &media_stream_output,
                &media_profile,
            )?
            .get()?;

        println!("transcoder prepared");

        let transcode_thread = std::thread::spawn({
            move || -> Result<(), Error> {
                println!("Starting transcoding...");
                let transcode_async = transcode.TranscodeAsync()?;
                println!("TranscodeAsync called");

                match transcode_async.get() {
                    Ok(_) => println!("Transcoding completed successfully"),
                    Err(e) => {
                        println!("Transcoding failed: {:?}", e);
                        return Err(e.into());
                    }
                }

                Ok(())
            }
        });

        println!("transcoder thread running");

        Ok(Self {
            sample_tx,
            sample_requested,
            media_stream_source,
            starting,
            transcode_thread: Some(transcode_thread),
        })
    }
}

impl Encoder for WmfEncoder {
    fn append_audio(&mut self, audio_sample: crate::AudioSample) -> Result<(), Error> {
        // TOCHECK: this might be wrong, need to double check
        let ts_delta_nanos = audio_sample.pts.as_nanos() as i64;

        let timespan = TimeSpan {
            Duration: ts_delta_nanos / 100,
        };

        let buffer = CryptographicBuffer::CreateFromByteArray(&audio_sample.data)?;
        let media_sample = MediaStreamSample::CreateFromBuffer(&buffer, timespan)?;

        self.sample_tx
            .send(Some(media_sample))
            .expect("couldn't send sample");
        println!("sample sent to encoder w ts: {}", ts_delta_nanos);

        Ok(())
    }

    fn finish(&mut self) -> Result<(), anyhow::Error> {
        println!("Finishing encoder...");

        // Send empty sample
        self.sample_tx.send(None).expect("couldn't send no-op");

        // Conclude transcode thread
        println!("Waiting for transcode thread...");
        if let Some(transcode_thread) = self.transcode_thread.take() {
            transcode_thread
                .join()
                .expect("Failed to join transcode thread")?;
        }

        // Close out stream source
        self.media_stream_source.RemoveStarting(self.starting)?;
        self.media_stream_source
            .RemoveSampleRequested(self.sample_requested)?;

        println!("Encoder finished successfully");

        Ok(())
    }
}
