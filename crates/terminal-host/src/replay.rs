use std::collections::VecDeque;
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReplayChunk {
    pub sequence: u64,
    pub data: Vec<u8>,
}
pub(crate) struct ReplayBuffer {
    chunks: VecDeque<ReplayChunk>,
    bytes: usize,
    limit: usize,
    next_sequence: u64,
}
impl ReplayBuffer {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            bytes: 0,
            limit,
            next_sequence: 1,
        }
    }
    pub(crate) fn push(&mut self, mut data: Vec<u8>) -> ReplayChunk {
        if data.len() > self.limit {
            data.drain(..data.len() - self.limit);
        }
        let chunk = ReplayChunk {
            sequence: self.next_sequence,
            data,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.bytes += chunk.data.len();
        self.chunks.push_back(chunk.clone());
        while self.bytes > self.limit {
            let Some(removed) = self.chunks.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(removed.data.len());
        }
        chunk
    }
    pub(crate) fn snapshot(&self) -> Vec<ReplayChunk> {
        self.chunks.iter().cloned().collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_is_ordered_and_bounded() {
        let mut replay = ReplayBuffer::new(5);
        assert_eq!(replay.push(vec![1, 2, 3]).sequence, 1);
        assert_eq!(replay.push(vec![4, 5, 6]).sequence, 2);
        assert_eq!(
            replay.snapshot(),
            vec![ReplayChunk {
                sequence: 2,
                data: vec![4, 5, 6],
            },],
        );
    }
    #[test]
    fn oversized_chunk_keeps_its_tail() {
        let mut replay = ReplayBuffer::new(3);
        replay.push(vec![1, 2, 3, 4, 5]);
        assert_eq!(replay.snapshot()[0].data, vec![3, 4, 5]);
    }
}
