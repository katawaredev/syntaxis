const COMMAND_MARKER_PREFIX: &[u8] = b"\x1b]777;syntaxis;";
const COMMAND_MARKER_END: u8 = 0x07;
const MAX_COMMAND_MARKER_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CommandMarker {
    Started,
    Finished(i32),
}

#[derive(Default)]
pub(super) struct CommandMarkerParser {
    pending: Vec<u8>,
}

impl CommandMarkerParser {
    pub(super) fn push(&mut self, data: &[u8]) -> (Vec<u8>, Vec<CommandMarker>) {
        self.pending.extend_from_slice(data);
        let mut visible = Vec::new();
        let mut markers = Vec::new();
        loop {
            let Some(start) = find_bytes(&self.pending, COMMAND_MARKER_PREFIX) else {
                let keep = partial_prefix_len(&self.pending, COMMAND_MARKER_PREFIX);
                let emit = self.pending.len().saturating_sub(keep);
                visible.extend(self.pending.drain(..emit));
                break;
            };
            visible.extend(self.pending.drain(..start));
            let Some(end_offset) = self.pending[COMMAND_MARKER_PREFIX.len()..]
                .iter()
                .position(|byte| *byte == COMMAND_MARKER_END)
            else {
                if self.pending.len() > MAX_COMMAND_MARKER_BYTES {
                    visible.extend(self.pending.drain(..1));
                    continue;
                }
                break;
            };
            let content_start = COMMAND_MARKER_PREFIX.len();
            let content_end = content_start + end_offset;
            if let Some(marker) = parse_command_marker(&self.pending[content_start..content_end]) {
                markers.push(marker);
            }
            self.pending.drain(..=content_end);
        }
        (visible, markers)
    }
}

fn parse_command_marker(content: &[u8]) -> Option<CommandMarker> {
    if content == b"command-start" {
        return Some(CommandMarker::Started);
    }
    std::str::from_utf8(content)
        .ok()?
        .strip_prefix("command-end;")?
        .parse::<i32>()
        .ok()
        .map(CommandMarker::Finished)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}

fn partial_prefix_len(data: &[u8], prefix: &[u8]) -> usize {
    (1..prefix.len().min(data.len()))
        .rev()
        .find(|length| data.ends_with(&prefix[..*length]))
        .unwrap_or(0)
}
