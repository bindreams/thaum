//! Unit tests for [`BufferedFile`](super::BufferedFile).

use std::io::{Read, Seek, Write};

use super::BufferedFile;

skuld::default_labels!(exec);

/// Create a BufferedFile backed by a tempfile with the given data.
fn bf_with_data(data: &[u8], passthrough: bool) -> BufferedFile {
    let mut tmp = tempfile::tempfile().expect("failed to create tempfile");
    tmp.write_all(data).unwrap();
    tmp.rewind().unwrap();
    if passthrough {
        BufferedFile::passthrough(tmp)
    } else {
        BufferedFile::new(tmp)
    }
}

#[skuld::test]
fn new_creates_buffered_reader() {
    let mut bf = bf_with_data(b"hello world", false);
    let mut buf = Vec::new();
    bf.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"hello world");
}

#[skuld::test]
fn passthrough_creates_unbuffered_reader() {
    let mut bf = bf_with_data(b"hello world", true);
    let mut buf = Vec::new();
    bf.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"hello world");
}

#[skuld::test]
fn into_inner_returns_file() {
    let bf = bf_with_data(b"test", false);
    let mut file = bf.into_inner();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"test");
}

#[skuld::test]
fn try_clone_returns_working_duplicate() {
    let bf = bf_with_data(b"clone me", false);
    let mut cloned = bf.try_clone().unwrap();
    let mut buf = Vec::new();
    cloned.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"clone me");
}

#[skuld::test]
fn write_delegates_to_inner_file() {
    let mut tmp = tempfile::tempfile().expect("failed to create tempfile");
    tmp.write_all(b"").unwrap();
    let mut bf = BufferedFile::new(tmp);
    bf.write_all(b"written").unwrap();
    bf.flush().unwrap();

    // Read back via the inner file.
    let mut file = bf.into_inner();
    file.rewind().unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"written");
}

#[skuld::test]
fn read_small_data_uses_buffer_refill() {
    // Data smaller than 8KB: should be served from the internal buffer.
    let data = vec![b'a'; 100];
    let mut bf = bf_with_data(&data, false);

    // Read in two small chunks — both should come from the buffer.
    let mut chunk1 = [0u8; 50];
    let n = bf.read(&mut chunk1).unwrap();
    assert_eq!(n, 50);
    assert_eq!(&chunk1[..], &data[..50]);

    let mut chunk2 = [0u8; 50];
    let n = bf.read(&mut chunk2).unwrap();
    assert_eq!(n, 50);
    assert_eq!(&chunk2[..], &data[50..100]);
}

#[skuld::test]
fn read_large_buffer_bypasses_internal_buffer() {
    // When dest.len() >= 8192, BufferedFile reads directly from file.
    let data = vec![b'b'; 16384];
    let mut bf = bf_with_data(&data, false);

    let mut dest = vec![0u8; 16384];
    let mut total = 0;
    while total < data.len() {
        let n = bf.read(&mut dest[total..]).unwrap();
        if n == 0 {
            break;
        }
        total += n;
    }
    assert_eq!(total, 16384);
    assert_eq!(&dest[..], &data[..]);
}

#[skuld::test]
fn read_partial_consumption_from_buffer() {
    // Read part of the buffer, then read the rest.
    let data = vec![b'c'; 200];
    let mut bf = bf_with_data(&data, false);

    // Read 10 bytes (triggers 8KB refill, serves 10 from buffer).
    let mut small = [0u8; 10];
    let n = bf.read(&mut small).unwrap();
    assert_eq!(n, 10);
    assert_eq!(&small[..], &data[..10]);

    // Read remaining — should come from the already-filled buffer.
    let mut rest = Vec::new();
    bf.read_to_end(&mut rest).unwrap();
    assert_eq!(rest.len(), 190);
    assert_eq!(&rest[..], &data[10..]);
}

#[skuld::test]
fn passthrough_read_no_buffering() {
    let data = vec![b'd'; 100];
    let mut bf = bf_with_data(&data, true);

    // Each read goes directly to the OS, no internal buffer.
    let mut chunk = [0u8; 50];
    let n = bf.read(&mut chunk).unwrap();
    assert!(n > 0 && n <= 50);

    let mut rest = Vec::new();
    bf.read_to_end(&mut rest).unwrap();
    assert_eq!(n + rest.len(), 100);
}
