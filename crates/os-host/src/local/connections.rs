use super::*;

impl HostCommandPlane {
    /// Ask the running Alan OS Host to reload Connection metadata and rebuild callables/catalogs.
    pub async fn reload_connections(&self) -> Result<()> {
        self.call(LocalRequest::ReloadConnections).await?;
        Ok(())
    }
}

pub(super) async fn write_local_request(
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    request: &LocalRequest,
) -> Result<()> {
    let bytes = serde_json::to_vec(request)?;
    ensure!(
        bytes.len() <= MAX_LOCAL_REQUEST_BYTES,
        "Host request is too large"
    );
    writer.write_u32(bytes.len() as u32).await?;
    writer.write_all(&bytes).await?;
    Ok(())
}

pub(super) async fn read_local_request(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
) -> Result<LocalRequest> {
    let length = reader.read_u32().await? as usize;
    ensure!(
        length <= MAX_LOCAL_REQUEST_BYTES,
        "Host request is too large"
    );
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes).context("decode Host request")
}

pub(super) async fn write_local_response(
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    response: LocalResponse,
) -> Result<()> {
    let bytes = serde_json::to_vec(&response)?;
    writer.write_u32(bytes.len() as u32).await?;
    writer.write_all(&bytes).await?;
    Ok(())
}

pub(super) async fn read_local_response(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
) -> Result<LocalResponse> {
    let length = reader.read_u32().await? as usize;
    ensure!(
        length <= MAX_LOCAL_REQUEST_BYTES,
        "Host response is too large"
    );
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes).context("decode Host response")
}
