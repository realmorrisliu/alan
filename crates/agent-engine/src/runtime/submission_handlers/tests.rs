use tokio_util::sync::CancellationToken;

async fn handle_runtime_op_with_cancel<E, F>(
    state: &mut super::super::transition::RuntimeLoopState,
    op: alan_agent_protocol::Op,
    emit: &mut E,
    _cancel: &CancellationToken,
) -> anyhow::Result<super::RuntimeOpAction>
where
    E: FnMut(alan_agent_protocol::Event) -> F,
    F: std::future::Future<Output = ()>,
{
    super::super::transition::handle_runtime_op(state, op, emit).await
}

include!("tests/confirmation_and_mount_support.inc.rs");
include!("tests/mount_and_user_input_contract.inc.rs");
include!("tests/runtime_ops_and_replay_contract.inc.rs");
