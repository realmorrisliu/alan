//! Byte/offset stream semantics (substrate §5.3): retained history, resume from
//! a caller-held offset, and a read that blocks until new bytes arrive — with no
//! separate notification primitive (§ "observation is a blocking read"). This is
//! the reusable primitive every file server's stream files build on, so the
//! "no missed / no mis-replayed records" guarantee is verified once here.

use std::time::Duration;

use alan_ap::Stream;

#[tokio::test]
async fn read_returns_all_bytes_from_offset_zero() {
    let stream = Stream::new();
    stream.append(b"hello ").await;
    stream.append(b"world").await;

    let bytes = stream.read(0, 64).await;
    assert_eq!(bytes, b"hello world");
}

#[tokio::test]
async fn read_resumes_from_a_caller_held_offset() {
    let stream = Stream::new();
    stream.append(b"hello world").await;

    // A reader holding offset 6 sees only the tail; it neither re-reads nor
    // misses the earlier bytes.
    let tail = stream.read(6, 64).await;
    assert_eq!(tail, b"world");
}

#[tokio::test]
async fn late_reader_still_reads_retained_history_from_zero() {
    let stream = Stream::new();
    // All records are produced before the reader ever opens.
    stream.append(b"record-1\n").await;
    stream.append(b"record-2\n").await;

    let replay = stream.read(0, 1024).await;
    assert_eq!(replay, b"record-1\nrecord-2\n");
}

#[tokio::test]
async fn read_blocks_until_new_bytes_are_appended() {
    let stream = Stream::new();
    let reader = stream.clone();

    // Reader parks at the live edge (offset 0, empty stream): it must not return
    // empty, it must wait for the first bytes.
    let handle = tokio::spawn(async move { reader.read(0, 64).await });

    // Give the reader time to block, then produce.
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        !handle.is_finished(),
        "read returned before any bytes were appended"
    );

    stream.append(b"late").await;
    let got = tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .expect("read did not wake within timeout")
        .expect("reader task panicked");
    assert_eq!(got, b"late");
}

// §5.5 — a failure that occurs after an interaction has begun streaming is a
// terminal record in the stream itself, observed by reading, not via a side
// channel. Here record typing is the consumer convention (one line per record).
#[tokio::test]
async fn mid_interaction_failure_is_a_terminal_record_in_the_stream() {
    let stream = Stream::new();
    stream.append(b"chunk-1\n").await;
    stream.append(b"chunk-2\n").await;
    // The interaction fails mid-flight; the failure is appended as a record.
    stream.append(b"error: upstream reset\n").await;

    let all = stream.read(0, 1024).await;
    let text = String::from_utf8(all).unwrap();
    let last = text.lines().last().unwrap();
    assert_eq!(last, "error: upstream reset");
}

#[tokio::test]
async fn many_readers_each_hold_their_own_offset() {
    let stream = Stream::new();
    stream.append(b"abcdef").await;

    let a = stream.read(0, 64).await;
    let b = stream.read(3, 64).await;
    assert_eq!(a, b"abcdef");
    assert_eq!(b, b"def");
}
