//! What the engine renders into, and what it renders it for.
//!
//! Audio is **deinterleaved f32, always** — device formats (i16, u16,
//! interleaved) convert at the driver edge and nowhere else. The planes live in
//! one flat buffer rather than a slice of slices, because a slice of mutable
//! slices cannot be built in a real-time callback without allocating, and the
//! engine may not allocate. (A deviation in shape from eng-01 §5's sketch; the
//! semantics — deinterleaved f32, channel count not baked in — are unchanged.)

use crate::time::SampleTime;

/// Everything the engine needs to know about the world it renders into.
///
/// **Sample rate is told, never assumed.** There is no rate constant anywhere in
/// this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub sample_rate: u32,
    pub channel_out: u16,
    /// 0 on an output-only stream — an HDMI endpoint has no input, and inventing
    /// one would be worse than not having it (eng-01 §11.3).
    pub channel_in: u16,
    /// The largest block the driver may ask for. Sizes the pre-allocation; the
    /// engine handles anything up to it, including a different size each
    /// callback — assuming a fixed block is the classic bug.
    pub max_block: u32,
}

impl Format {
    pub fn stereo(sample_rate: u32, max_block: u32) -> Format {
        Format {
            sample_rate,
            channel_out: 2,
            channel_in: 0,
            max_block,
        }
    }

    pub fn is_duplex(self) -> bool {
        self.channel_in > 0
    }
}

/// A read-only planar view: `channel` planes of `frame` samples, each plane
/// `stride` apart.
#[derive(Clone, Copy)]
pub struct Planar<'a> {
    data: &'a [f32],
    stride: usize,
    channel: usize,
    frame: usize,
}

impl<'a> Planar<'a> {
    pub fn new(data: &'a [f32], stride: usize, channel: usize, frame: usize) -> Planar<'a> {
        debug_assert!(data.len() >= stride * channel);
        debug_assert!(frame <= stride);
        Planar {
            data,
            stride,
            channel,
            frame,
        }
    }

    pub fn channel(&self) -> usize {
        self.channel
    }

    pub fn plane(&self, channel: usize) -> &[f32] {
        &self.data[channel * self.stride..][..self.frame]
    }
}

/// A writable planar view. The engine's output surface.
pub struct PlanarMut<'a> {
    data: &'a mut [f32],
    stride: usize,
    channel: usize,
    frame: usize,
}

impl<'a> PlanarMut<'a> {
    pub fn new(data: &'a mut [f32], stride: usize, channel: usize, frame: usize) -> PlanarMut<'a> {
        debug_assert!(data.len() >= stride * channel);
        debug_assert!(frame <= stride);
        PlanarMut {
            data,
            stride,
            channel,
            frame,
        }
    }

    pub fn channel(&self) -> usize {
        self.channel
    }

    pub fn frame(&self) -> usize {
        self.frame
    }

    pub fn plane(&mut self, channel: usize) -> &mut [f32] {
        &mut self.data[channel * self.stride..][..self.frame]
    }

    /// A window into one plane, for rendering a sub-range between two events.
    pub fn segment(&mut self, channel: usize, from: usize, to: usize) -> &mut [f32] {
        &mut self.data[channel * self.stride + from..][..to - from]
    }

    /// Copy one plane over another. A method rather than two `plane` calls,
    /// because two mutable borrows of the same buffer is exactly what the
    /// borrow checker is for.
    pub fn copy_plane(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        let (low, high) = (from.min(to), from.max(to));
        let (head, tail) = self.data.split_at_mut(high * self.stride);
        let (source, target) = if from < to {
            (
                &head[low * self.stride..][..self.frame],
                &mut tail[..self.frame],
            )
        } else {
            // `split_at_mut` always puts the lower index in `head`, so the
            // direction of the copy is what swaps, not the split.
            let (target, source) = (
                &mut head[low * self.stride..][..self.frame],
                &tail[..self.frame],
            );
            return target.copy_from_slice(source);
        };
        target.copy_from_slice(source);
    }

    /// Peak absolute value in a plane, for metering.
    pub fn peak(&self, channel: usize) -> f32 {
        self.data[channel * self.stride..][..self.frame]
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
    }

    pub fn silence(&mut self) {
        for channel in 0..self.channel {
            self.plane(channel).fill(0.0);
        }
    }
}

/// One block of work.
pub struct Block<'a> {
    /// `None` on an output-only stream. Present from day one so that adding
    /// input later is not a signature change through every node that will ever
    /// exist (eng-01 §5).
    pub inp: Option<Planar<'a>>,
    pub out: PlanarMut<'a>,
    /// Sample position of frame 0 on the engine timeline.
    pub at: SampleTime,
}

impl Block<'_> {
    pub fn frame(&self) -> usize {
        self.out.frame()
    }
}
