//! The cpal-backed driver: a real device, a real callback.
//!
//! **Selection is an act, not an inheritance** (eng-01 §11.1). "The default
//! device" means whatever the OS last decided — plug in an interface and it may
//! switch silently — so a request names what it wants, the resolution is
//! reported, and the report is what gets logged.
//!
//! **The stream opens once and never stops.** Silence is written when the
//! transport is stopped; the device never learns that stop was pressed. Four
//! reasons, any one sufficient: starting a stream costs milliseconds; the sample
//! clock's continuity would break; R-1512 says nothing gates the start; R-1513
//! says the application is playable on launch. And a practical fifth — many
//! televisions mute their amplifier during silence and take 100–300 ms to
//! unmute, so a stream-per-transport design would swallow the first note and
//! look exactly like a scheduling bug.
//!
//! ## A finding, recorded rather than discovered later
//!
//! **cpal has no duplex callback.** Input and output are separate streams with
//! separate callbacks, even on one device. So the single-device rule of eng-01
//! §11.3 is necessary but not sufficient: a real live path will additionally
//! need the two callbacks reconciled against one clock. Since input is not yet
//! used, this driver opens **output only** and says so in its report — which is
//! exactly the "output-only permitted and recorded" case, arrived at for a
//! second reason nobody anticipated.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::engine::Engine;
use crate::error::EngineError;
use crate::format::{Block, Format, PlanarMut};
use crate::port::RtPort;

/// Environment override for device selection, until there is a settings file
/// (eng-01 §11.2). Matched case-insensitively as a substring of the device name.
pub const DEVICE_ENV: &str = "REVISION_AUDIO_DEVICE";

/// What the caller wants. Every field is a preference, not a demand — the
/// report says what was actually resolved.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// Substring of the device name. `None` takes the environment override if
    /// set, and the system default otherwise.
    pub name: Option<String>,
    pub sample_rate: Option<u32>,
    /// Frames per callback. `None` lets the host choose, which on shared-mode
    /// WASAPI is the honest option.
    pub block: Option<u32>,
}

/// What actually happened. Everything here is logged at open — a device whose
/// choice was silent is a device you cannot debug.
#[derive(Debug, Clone)]
pub struct OpenReport {
    pub device: String,
    pub host: String,
    pub format: Format,
    /// False here always, for now: see the module note on cpal and duplex.
    pub duplex: bool,
    /// Whether the host supplies callback timestamps, or we fall back to
    /// reading the monotonic clock at callback entry. Recorded rather than
    /// assumed, because it changes what the correlation is worth.
    pub driver_timestamps: bool,
    pub requested_block: Option<u32>,
}

impl OpenReport {
    /// The line a human reads in the log. Prose, not a code (eng-01 §9.5).
    pub fn summary(&self) -> String {
        format!(
            "stream open: {} via {}, {} Hz, {} ch, {} frames max, {}, timestamps from {}",
            self.device,
            self.host,
            self.format.sample_rate,
            self.format.channel_out,
            self.format.max_block,
            if self.duplex { "duplex" } else { "output only" },
            if self.driver_timestamps {
                "driver"
            } else {
                "monotonic clock"
            },
        )
    }
}

/// A live stream. Dropping it closes the device.
pub struct Device {
    _stream: cpal::Stream,
    report: OpenReport,
}

impl Device {
    pub fn report(&self) -> &OpenReport {
        &self.report
    }

    /// Open a stream and start it. The engine runs from this moment until the
    /// `Device` is dropped, whatever the transport is doing.
    pub fn open(request: &Request, port: RtPort) -> Result<Device, EngineError> {
        let host = cpal::default_host();
        let wanted = request
            .name
            .clone()
            .or_else(|| std::env::var(DEVICE_ENV).ok());

        let device = match &wanted {
            Some(fragment) => find_device(&host, fragment)?,
            None => host
                .default_output_device()
                .ok_or(EngineError::NoDevice { wanted: None })?,
        };
        // cpal 0.18 replaced `Device::name()` with `Display` plus a structured
        // `description()`. The display name is what a human recognizes.
        let name = device.to_string();

        let supported = device
            .default_output_config()
            .map_err(|source| EngineError::Config {
                device: name.clone(),
                detail: source.to_string(),
            })?;

        let mut config: cpal::StreamConfig = supported.config();
        if let Some(rate) = request.sample_rate {
            config.sample_rate = rate;
        }
        if let Some(block) = request.block {
            config.buffer_size = cpal::BufferSize::Fixed(block);
        }

        let channel = config.channels;
        let sample_rate = config.sample_rate;
        // The host may hand us any block size, including a different one each
        // callback. `max_block` sizes the pre-allocation generously; anything
        // larger is processed in several passes rather than reallocating.
        let max_block = request.block.unwrap_or(2048).max(2048);
        let format = Format {
            sample_rate,
            channel_out: channel,
            channel_in: 0,
            max_block,
        };

        let mut engine = Engine::new(format, port);
        // The only latency the engine can honestly report is the device's own
        // (R-310). Not measured here — cpal does not tell us — so it is left at
        // zero rather than guessed at.
        engine.set_device_latency(0, 0);

        let mut scratch = vec![0.0f32; max_block as usize * channel as usize];
        let stride = max_block as usize;
        // The error callback runs on an unspecified thread and cannot touch the
        // engine, so xruns are counted here and folded in on the next callback.
        let pending_xrun = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let xrun_flag = Arc::clone(&pending_xrun);

        let error_name = name.clone();
        let stream = device
            .build_output_stream(
                config,
                move |out: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    let taken = xrun_flag.swap(0, std::sync::atomic::Ordering::Relaxed);
                    for _ in 0..taken {
                        engine.note_xrun();
                    }
                    fill(&mut engine, &mut scratch, stride, channel as usize, out);
                },
                move |error| {
                    // Nothing here may allocate a log line into the engine's
                    // rings — this is not the audio thread and does not own
                    // them. Stderr is the honest channel; the device-lost policy
                    // (transport stops, silence, visible state) belongs to the
                    // app, which sees the stream end.
                    eprintln!("rev-engine: stream error on {error_name}: {error}");
                },
                None,
            )
            .map_err(|source| EngineError::Build {
                device: name.clone(),
                detail: source.to_string(),
            })?;

        stream.play().map_err(|source| EngineError::Build {
            device: name.clone(),
            detail: source.to_string(),
        })?;

        Ok(Device {
            _stream: stream,
            report: OpenReport {
                device: name,
                host: host.id().name().to_string(),
                format,
                duplex: false,
                driver_timestamps: false,
                requested_block: request.block,
            },
        })
    }

    /// Every output device the host can see, for a chooser — and for a log line
    /// when the requested one is absent.
    pub fn list() -> Vec<String> {
        let host = cpal::default_host();
        host.output_devices()
            .map(|devices| devices.map(|device| device.to_string()).collect())
            .unwrap_or_default()
    }
}

fn find_device(host: &cpal::Host, fragment: &str) -> Result<cpal::Device, EngineError> {
    let wanted = fragment.to_lowercase();
    let devices = host
        .output_devices()
        .map_err(|source| EngineError::Config {
            device: fragment.to_string(),
            detail: source.to_string(),
        })?;
    for device in devices {
        if device.to_string().to_lowercase().contains(&wanted) {
            return Ok(device);
        }
    }
    Err(EngineError::NoDevice {
        wanted: Some(fragment.to_string()),
    })
}

/// The callback body: render into planar scratch, then interleave into the
/// device's buffer.
///
/// Split out of the closure so it can be read — and so the one place that
/// touches raw device memory is one screen long.
fn fill(engine: &mut Engine, scratch: &mut [f32], stride: usize, channel: usize, out: &mut [f32]) {
    let total = out.len() / channel;
    let mut done = 0;
    while done < total {
        let frame = (total - done).min(stride);
        let at = engine.at();
        let mut block = Block {
            inp: None,
            out: PlanarMut::new(scratch, stride, channel, frame),
            at,
        };
        engine.process(&mut block);

        for c in 0..channel {
            let plane = &scratch[c * stride..][..frame];
            for (i, sample) in plane.iter().enumerate() {
                out[(done + i) * channel + c] = *sample;
            }
        }
        done += frame;
    }
}
