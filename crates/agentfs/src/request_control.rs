//! Request-owner control; called under the same state lock as response settlement.
use super::{Node, State, is_terminal};
use alan_ap::ErrorCode;

impl State {
    pub(super) async fn write_request_control(
        &mut self,
        id: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<u32, ErrorCode> {
        if offset != 0 || data != b"cancel" {
            return Err(ErrorCode::BadRequest);
        }
        self.cancel_request(id).await?;
        Ok(data.len() as u32)
    }

    pub(super) async fn cancel_request(&mut self, id: &str) -> Result<(), ErrorCode> {
        let request = self.requests.get_mut(id).ok_or(ErrorCode::NotFound)?;
        if request.status == "pending" {
            request.status = "cancelled".to_string();
            self.bump(&Node::RequestField(id.to_string(), "status"));
            self.request_events
                .append(format!("{id}:status\n").as_bytes())
                .await;
            self.events
                .append(format!("request:{id}\n").as_bytes())
                .await;
        } else if !is_terminal(&request.status) {
            return Err(ErrorCode::BadRequest);
        }
        Ok(())
    }
}
