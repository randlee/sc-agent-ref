//! Private helpers of the stream module: line reading, id rules, line split.

use std::io::Cursor;

use super::*;

fn read_all(data: &[u8], max: usize) -> Vec<(&'static str, Vec<u8>)> {
    let mut reader = Cursor::new(data.to_vec());
    let mut out = Vec::new();
    let mut buffer = Vec::new();
    loop {
        match read_line_bounded(&mut reader, max, &mut buffer).expect("read") {
            LineRead::End => return out,
            LineRead::Line => out.push(("line", buffer.clone())),
            LineRead::TooLong => out.push(("long", Vec::new())),
        }
    }
}

#[test]
fn lines_are_split_and_a_final_line_without_newline_is_kept() {
    let got = read_all(b"ab\ncd\nef", 64);
    assert_eq!(
        got,
        vec![
            ("line", b"ab\n".to_vec()),
            ("line", b"cd\n".to_vec()),
            ("line", b"ef".to_vec()),
        ]
    );
}

#[test]
fn an_over_long_line_is_dropped_whole_and_the_next_line_survives() {
    let mut data = vec![b'x'; 50];
    data.push(b'\n');
    data.extend_from_slice(b"ok\n");
    let got = read_all(&data, 10);
    assert_eq!(got, vec![("long", Vec::new()), ("line", b"ok\n".to_vec())]);
}

/// The buffer never holds more than the cap, however long the line is.
#[test]
fn reading_never_buffers_more_than_the_cap() {
    let data = vec![b'x'; 10_000];
    let mut reader = Cursor::new(data);
    let mut buffer = Vec::new();
    let read = read_line_bounded(&mut reader, 100, &mut buffer).expect("read");
    assert!(matches!(read, LineRead::TooLong));
    assert!(buffer.len() <= 100);
}

#[test]
fn ids_are_bounded_and_printable() {
    assert!(check_id("q-1").is_ok());
    assert!(check_id(&"a".repeat(MAX_ID_BYTES)).is_ok());
    for bad in [
        String::new(),
        "a".repeat(MAX_ID_BYTES + 1),
        "a\nb".to_owned(),
    ] {
        let error = check_id(&bad).expect_err("bad id");
        assert_eq!(error.code(), "INPUT_INVALID");
        assert_eq!(error.field(), Some("id"));
    }
}

#[test]
fn a_line_splits_into_id_and_the_request_text_verbatim() {
    let (id, request) = split_line(r#"{"id":"a","request":{"questions": []}}"#).expect("split");
    assert_eq!(id, "a");
    assert_eq!(request, r#"{"questions": []}"#);
}

#[test]
fn a_bad_line_keeps_a_usable_id_and_never_an_unusable_one() {
    let BadLine { id, error } =
        *split_line(r#"{"id":"keep","request":{},"extra":1}"#).expect_err("extra");
    assert_eq!(id.as_deref(), Some("keep"));
    assert_eq!(error.code(), "INPUT_INVALID");
    let BadLine { id, .. } = *split_line(r#"{"id":"keep"}"#).expect_err("no request");
    assert_eq!(id.as_deref(), Some("keep"));
    let BadLine { id, .. } = *split_line("not json").expect_err("not json");
    assert_eq!(id, None);
    let BadLine { id, .. } = *split_line(r#"{"id":7,"request":{}}"#).expect_err("numeric id");
    assert_eq!(id, None);
    let long = format!(
        r#"{{"id":"{}","request":{{}}}}"#,
        "a".repeat(MAX_ID_BYTES + 1)
    );
    let BadLine { id, .. } = *split_line(&long).expect_err("long id");
    assert_eq!(id, None);
}

#[test]
fn concurrency_is_one_to_sixty_four_with_a_default_of_eight() {
    assert_eq!(Concurrency::default().get(), 8);
    assert_eq!(Concurrency::DEFAULT, Concurrency::new(8).expect("8"));
    assert!(Concurrency::new(1).is_ok());
    assert!(Concurrency::new(Concurrency::MAX).is_ok());
    for bad in [0, Concurrency::MAX + 1] {
        let error = Concurrency::new(bad).expect_err("out of range");
        assert_eq!(error.code(), "INPUT_INVALID");
        assert_eq!(error.field(), Some("concurrency"));
    }
}
