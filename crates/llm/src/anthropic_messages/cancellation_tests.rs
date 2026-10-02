use super::*;

#[tokio::test]
async fn downstream_drop_releases_silent_event_input() {
    let (input, events) = mpsc::channel(1);
    let output = project_events(events);
    drop(output);
    tokio::time::timeout(std::time::Duration::from_millis(500), input.closed())
        .await
        .expect("projector retained silent upstream after downstream drop");
}
