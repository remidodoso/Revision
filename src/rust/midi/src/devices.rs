//! Enumeration and hot-plug (midi-02, R-601).
//!
//! `midir` offers a *snapshot* of the ports that exist right now, not a
//! subscription — there is no cross-platform arrival/removal callback. So
//! hot-plug is polling: list the ports each UI frame, diff against last time,
//! and report what appeared and what vanished. Cheap (a handful of strings) and
//! honest about what the platform actually provides.
//!
//! **Identity is the port name** (R-602), which on Windows is often a surplus of
//! confusing names — that is the platform's, not ours to fix. Persisting a
//! project's chosen device across sessions is a settings-store concern and is
//! deferred until that store exists; here a device is only ever named.

/// What changed between two polls.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    /// Ports that appeared since the last poll.
    pub arrived: Vec<String>,
    /// Ports that vanished since the last poll.
    pub removed: Vec<String>,
}

impl Change {
    pub fn is_empty(&self) -> bool {
        self.arrived.is_empty() && self.removed.is_empty()
    }
}

/// A running view of the MIDI input ports, diffed on each poll for hot-plug.
#[derive(Debug, Clone, Default)]
pub struct Devices {
    present: Vec<String>,
}

impl Devices {
    pub fn new() -> Devices {
        Devices::default()
    }

    /// The ports present as of the last poll, in list order.
    pub fn ports(&self) -> &[String] {
        &self.present
    }

    /// Poll the real MIDI stack and report what changed.
    pub fn poll(&mut self) -> Change {
        self.update(crate::ports::list())
    }

    /// The diff logic, given a fresh list — separated so hot-plug is testable
    /// without plugging anything in.
    pub fn update(&mut self, now: Vec<String>) -> Change {
        let arrived = now
            .iter()
            .filter(|p| !self.present.contains(p))
            .cloned()
            .collect();
        let removed = self
            .present
            .iter()
            .filter(|p| !now.contains(p))
            .cloned()
            .collect();
        self.present = now;
        Change { arrived, removed }
    }

    /// The index of a port by name — what `ports::open` wants. `None` if it is
    /// not present, which is the graceful-rebind case: a remembered device that
    /// has not come back yet (R-602).
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.present.iter().position(|p| p == name)
    }
}

#[cfg(test)]
mod test;
