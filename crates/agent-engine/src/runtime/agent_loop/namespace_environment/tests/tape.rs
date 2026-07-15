use super::super::agent_files::tape_record_bytes;
use super::*;

#[tokio::test]
async fn engine_tape_writer_holds_generating_lease_and_allows_readers() {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

    let mut writer = environment.begin_tape_generation().await.unwrap();

    let second_writer = environment.begin_tape_generation().await;
    assert!(
        second_writer.is_err(),
        "a second engine writer must not acquire machine/tape while GENERATING lease is held"
    );

    let mut tape_tail = shell.tail("/agent/1/machine/tape").await.unwrap();
    writer.append_record("user", "hello").await.unwrap();
    let streamed = String::from_utf8(tape_tail.read(64 * 1024).await.unwrap()).unwrap();
    assert!(streamed.contains(r#""role":"user""#), "{streamed}");
    assert!(streamed.contains(r#""content":"hello""#), "{streamed}");
    tape_tail.close().await.unwrap();

    writer.append_record("assistant", "hi").await.unwrap();
    writer.finish().await.unwrap();

    let mut next_writer = environment.begin_tape_generation().await.unwrap();
    next_writer
        .append_record("assistant", "after lease")
        .await
        .unwrap();
    next_writer.finish().await.unwrap();

    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    assert!(tape.contains(r#""content":"hi""#), "{tape}");
    assert!(tape.contains(r#""content":"after lease""#), "{tape}");
}

#[test]
fn tape_record_shape_is_content_addressable_ready() {
    let record = tape_record_bytes("assistant", "stable text").unwrap();
    assert_eq!(
        String::from_utf8(record).unwrap(),
        r#"{"version":1,"kind":"message","role":"assistant","content":"stable text"}"#.to_string()
            + "\n",
        "tape records must stay canonical, self-contained newline-delimited units"
    );
}
