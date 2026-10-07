use super::*;
use axum::{
    body::Body,
    routing::{MethodRouter, get},
};

async fn failure(route: MethodRouter) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/models", route))
            .await
            .unwrap();
    });
    let temp = TempDir::new().unwrap();
    let path = seed_chatgpt_auth(
        temp.path().join("auth.json"),
        valid_access_token(),
        "refresh",
    );
    let result = test_client(&url, path).model_catalog().await;
    server.abort();
    result.unwrap_err().to_string()
}

#[tokio::test]
async fn catalog_rejects_oversize_declared_and_chunked_bodies() {
    let declared = get(|| async { "x".repeat(2 * 1024 * 1024 + 1) });
    let chunked = get(|| async {
        Body::from_stream(futures::stream::iter([
            Ok::<_, std::io::Error>(vec![b'x'; 1024 * 1024]),
            Ok(vec![b'x'; 1024 * 1024 + 1]),
        ]))
    });
    for route in [declared, chunked] {
        assert_eq!(
            failure(route).await,
            "ChatGPT model catalog exceeds size limit"
        );
    }
}

#[tokio::test]
async fn catalog_timeout_bounds_the_whole_discovery_operation() {
    let stalled = get(|| async {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        "never returned before the client deadline"
    });
    let error = tokio::time::timeout(std::time::Duration::from_secs(20), failure(stalled))
        .await
        .unwrap();
    assert_eq!(error, "ChatGPT model catalog timed out");
}
