//! Shared Files view tests.

use super::*;
#[test]
fn search_returns_non_overlapping_byte_ranges() {
    assert_eq!(
        find_matches("one two one", "one", SearchOptions::default()).unwrap(),
        vec![(0, 3), (8, 11)]
    );
}
#[test]
fn search_modes_handle_case_words_and_regex_errors() {
    let sensitive = SearchOptions {
        case_sensitive: true,
        ..SearchOptions::default()
    };
    assert_eq!(
        find_matches("Install install", "install", sensitive).unwrap(),
        vec![(8, 15)]
    );

    let whole_word = SearchOptions {
        whole_word: true,
        ..SearchOptions::default()
    };
    assert_eq!(
        find_matches("cat catalog cat_2 cat", "cat", whole_word).unwrap(),
        vec![(0, 3), (18, 21)]
    );

    let regex = SearchOptions {
        regex: true,
        ..SearchOptions::default()
    };
    find_matches("anything", "[", regex).expect_err("invalid regexes must be rejected");
}
#[test]
fn replacement_supports_literal_dollars_and_regex_captures() {
    assert_eq!(
        replace_search_match("cost $1", "$1", "$2", SearchOptions::default(), (5, 7),).unwrap(),
        "cost $2"
    );

    let regex = SearchOptions {
        regex: true,
        ..SearchOptions::default()
    };
    assert_eq!(
        replace_all_search_matches("Doe, Jane; Roe, Richard", r"(\w+), (\w+)", "$2 $1", regex,)
            .unwrap(),
        "Jane Doe; Richard Roe"
    );
}
#[test]
fn image_detection_is_explicit() {
    assert_eq!(
        crate::image_mime("assets/photo.PNG"),
        Some("image/png")
    );
    assert_eq!(crate::image_mime("archive.bin"), None);
}

#[test]
fn upload_body_limit_errors_are_readable() {
    let error = "HTTP 500: Server function panicked: FailedToBufferBody(LengthLimitError)";

    assert_eq!(
        upload_error_message("Memos.pdf", error),
        "Could not upload Memos.pdf: the server rejected the request as too large."
    );
}
