use super::upload_error_message;

#[test]
fn upload_body_limit_errors_are_readable() {
    let error = "HTTP 500: Server function panicked: FailedToBufferBody(LengthLimitError)";

    assert_eq!(
        upload_error_message("Memos.pdf", error),
        "Could not upload Memos.pdf: the server rejected the request as too large."
    );
}
