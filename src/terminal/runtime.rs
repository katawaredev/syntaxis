use dioxus::prelude::*;

use super::renderer::{RendererAction, RendererCommand, RendererOutput, RendererOutputBatch};

const INITIAL_RECONNECT_DELAY_MS: u64 = 250;
const MAX_RECONNECT_DELAY_MS: u64 = 8_000;
const MAX_RENDERER_REPLAY_BYTES: usize = 2 * 1024 * 1024;

pub(super) const MAX_RECONNECT_ATTEMPTS: u8 = 6;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ConnectionState {
    Connecting,
    Reconnecting {
        attempt: u8,
        delay_ms: u64,
        message: String,
    },
    Ready,
    Failed(String),
}

pub(super) fn fail_pending_requests(
    creating_session: &mut Signal<bool>,
    new_name_server_error: &mut Signal<Option<String>>,
    pending_command: &mut Signal<Option<String>>,
    toast: &mut Signal<Option<String>>,
) {
    if creating_session() {
        creating_session.set(false);
        new_name_server_error.set(Some(
            "The connection was interrupted. Review the session list and retry.".into(),
        ));
    }
    if pending_command().is_some() {
        pending_command.set(None);
        toast.set(Some(
            "The run command was interrupted. Review the session list and retry.".into(),
        ));
    }
}

pub(super) fn reconnect_delay_ms(attempt: u8) -> u64 {
    let exponent = attempt.saturating_sub(1).min(5);
    (INITIAL_RECONNECT_DELAY_MS * (1_u64 << exponent)).min(MAX_RECONNECT_DELAY_MS)
}

pub(super) fn command_input(command: &str, exit_after: bool) -> Vec<u8> {
    let suffix = if exit_after {
        "; __syntaxis_setup_status=$?; exit $__syntaxis_setup_status\n"
    } else {
        "\n"
    };
    let mut input = Vec::with_capacity(command.len() + suffix.len());
    input.extend_from_slice(command.as_bytes());
    input.extend_from_slice(suffix.as_bytes());
    input
}

pub(super) fn push_renderer_output(
    output: &mut Signal<Option<RendererOutputBatch>>,
    chunk: RendererOutput,
) {
    let mut current = output.write();
    if current
        .as_ref()
        .is_none_or(|batch| batch.session_id != chunk.session_id)
    {
        *current = Some(RendererOutputBatch::new(chunk.session_id.clone()));
    }
    if let Some(batch) = current.as_mut() {
        batch.push(chunk, MAX_RENDERER_REPLAY_BYTES);
    }
}

pub(super) fn send_renderer_action(
    command: &mut Signal<Option<RendererCommand>>,
    sequence: &mut Signal<u64>,
    action: RendererAction,
) {
    *sequence.write() = sequence().saturating_add(1);
    command.set(Some(RendererCommand {
        sequence: sequence(),
        action,
    }));
}

pub(super) fn server_error_message(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, .. } => message,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use syntaxis_terminal::SessionId;

    use super::*;

    #[test]
    fn reconnect_backoff_is_exponential_and_bounded() {
        assert_eq!(reconnect_delay_ms(1), 250);
        assert_eq!(reconnect_delay_ms(2), 500);
        assert_eq!(reconnect_delay_ms(6), 8_000);
        assert_eq!(reconnect_delay_ms(u8::MAX), 8_000);
    }

    #[test]
    fn initializer_exit_is_part_of_the_same_shell_line() {
        let input = String::from_utf8(command_input("npm create vite", true)).unwrap();
        assert_eq!(input.matches('\n').count(), 1);
        assert_eq!(
            input,
            "npm create vite; __syntaxis_setup_status=$?; exit $__syntaxis_setup_status\n"
        );
    }

    #[test]
    fn renderer_output_batch_evicts_old_chunks_without_crossing_sessions() {
        let session_id = SessionId::new("one");
        let mut batch = RendererOutputBatch::new(session_id.clone());
        batch.push(
            RendererOutput {
                session_id: session_id.clone(),
                sequence: 1,
                data: vec![1, 2, 3],
            },
            5,
        );
        batch.push(
            RendererOutput {
                session_id,
                sequence: 2,
                data: vec![4, 5, 6],
            },
            5,
        );
        assert_eq!(batch.chunks.len(), 1);
        assert_eq!(batch.chunks[0].sequence, 2);
    }
}
