//! Rendering without a device.
//!
//! Two jobs, and the second is the important one:
//!
//! 1. Produce audio faster than real time, for offline render (R-1402).
//! 2. **Run the engine under the allocation guard in CI**, on machines with no
//!    sound card. Every real-time invariant this project has is checked here,
//!    which is why the driver-agnostic core (eng-01 §5) earns its keep.

use crate::engine::Engine;
use crate::format::{Block, Format, PlanarMut};
use crate::port::RtPort;

pub struct Offline {
    engine: Engine,
    /// Planar scratch, `max_block` frames per channel. Allocated once, here,
    /// where allocating is allowed.
    scratch: Vec<f32>,
    block: usize,
}

impl Offline {
    pub fn new(format: Format, port: RtPort) -> Offline {
        let block = format.max_block as usize;
        let channel = format.channel_out as usize;
        Offline {
            scratch: vec![0.0; block * channel],
            block,
            engine: Engine::new(format, port),
        }
    }

    pub fn engine(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn format(&self) -> Format {
        self.engine.format()
    }

    /// Render `frame` frames, interleaved, in blocks of the format's
    /// `max_block`.
    ///
    /// Blocks matter: rendering 48000 frames as one call would not exercise the
    /// per-block command dispatch, and the whole point of an offline path that
    /// shares the core is that it exercises the same code.
    pub fn render(&mut self, frame: usize) -> Vec<f32> {
        let channel = self.format().channel_out as usize;
        let mut out = vec![0.0f32; frame * channel];
        self.render_into(&mut out);
        out
    }

    /// Render into a caller's interleaved buffer, whose length decides how many
    /// frames are produced.
    pub fn render_into(&mut self, out: &mut [f32]) {
        let channel = self.format().channel_out as usize;
        let total = out.len() / channel;
        let mut done = 0;

        while done < total {
            let frame = (total - done).min(self.block);
            let at = self.engine.at();
            let mut block = Block {
                inp: None,
                out: PlanarMut::new(&mut self.scratch, self.block, channel, frame),
                at,
            };
            self.engine.process(&mut block);

            // Interleave. Outside `process`, so the guard is disarmed and this
            // is ordinary code — but it still allocates nothing.
            for c in 0..channel {
                let plane = &self.scratch[c * self.block..][..frame];
                for (i, sample) in plane.iter().enumerate() {
                    out[(done + i) * channel + c] = *sample;
                }
            }
            done += frame;
        }
    }
}

/// Write interleaved f32 samples as a WAV file.
///
/// Hand-rolled rather than a dependency: a 32-bit float WAV header is 44 bytes
/// of well-documented structure, and this is the only audio file format the
/// project needs until audio import exists.
pub fn write_wav(
    path: &std::path::Path,
    samples: &[f32],
    sample_rate: u32,
    channel: u16,
) -> std::io::Result<()> {
    use std::io::Write;

    let bytes = (samples.len() * 4) as u32;
    let byte_rate = sample_rate * u32::from(channel) * 4;
    let block_align = channel * 4;

    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?; // PCM header size
    file.write_all(&3u16.to_le_bytes())?; // 3 = IEEE float
    file.write_all(&channel.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&32u16.to_le_bytes())?; // bits per sample
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    file.flush()
}
