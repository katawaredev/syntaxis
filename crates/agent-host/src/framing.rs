//! Bounded line framing for Pi's newline-delimited JSON protocol.

#[derive(Debug, Eq, PartialEq)]
pub(super) enum FramedLine {
    Line(Vec<u8>),
    Oversized,
}

pub(super) struct BoundedLfFramer {
    limit: usize,
    record: Vec<u8>,
    oversized: bool,
}

impl BoundedLfFramer {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            record: Vec::with_capacity(limit.min(8 * 1024)),
            oversized: false,
        }
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Vec<FramedLine> {
        let mut frames = Vec::new();
        for byte in chunk {
            if *byte == b'\n' {
                if let Some(frame) = self.complete() {
                    frames.push(frame);
                }
            } else if !self.oversized {
                if self.record.len() >= self.limit {
                    self.record.clear();
                    self.oversized = true;
                } else {
                    self.record.push(*byte);
                }
            }
        }
        frames
    }

    pub(super) fn finish(&mut self) -> Option<FramedLine> {
        self.complete()
    }

    fn complete(&mut self) -> Option<FramedLine> {
        if self.oversized {
            self.oversized = false;
            self.record.clear();
            return Some(FramedLine::Oversized);
        }
        if self.record.last() == Some(&b'\r') {
            self.record.pop();
        }
        if self.record.is_empty() {
            return None;
        }
        Some(FramedLine::Line(std::mem::take(&mut self.record)))
    }
}
