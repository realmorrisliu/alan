use super::*;

// [DONE], not a finish-bearing delta or payload presence, acknowledges transport completion.
pub(super) async fn consume_chat_bytes<S>(
    mut stream: S,
    tx: tokio::sync::mpsc::Sender<OpenAiChatCompletionsChunk>,
) -> Result<()>
where
    S: futures::Stream<Item = Result<Vec<u8>>> + Unpin,
{
    let mut parser = SseEventParser::new();
    loop {
        let chunk = tokio::select! {
            _ = tx.closed() => return Ok(()),
            chunk = stream.next() => match chunk { Some(chunk) => chunk, None => break },
        };
        for data in parser.push(&chunk?) {
            if data.trim() == "[DONE]" {
                return Ok(());
            }
            let chunk = serde_json::from_str(&data)?;
            if tx.send(chunk).await.is_err() {
                return Ok(()); // Cancellation: no remaining receiver to notify.
            }
        }
    }
    for data in parser.finish() {
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let chunk = serde_json::from_str(&data)?;
        if tx.send(chunk).await.is_err() {
            return Ok(());
        }
    }
    Err(crate::failure::StreamClosed.into())
}
