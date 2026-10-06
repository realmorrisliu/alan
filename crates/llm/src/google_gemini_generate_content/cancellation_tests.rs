use super::*;

#[tokio::test]
async fn downstream_drop_releases_silent_gemini_input() {
    let (input, events) = tokio::sync::mpsc::channel(1);
    let (tx, output) = tokio::sync::mpsc::channel(1);
    let projector = tokio::spawn(project_chunks(events, tx));
    drop(output);
    tokio::time::timeout(std::time::Duration::from_millis(500), input.closed())
        .await
        .expect("projector retained silent upstream after downstream drop");
    projector.await.unwrap();
}
