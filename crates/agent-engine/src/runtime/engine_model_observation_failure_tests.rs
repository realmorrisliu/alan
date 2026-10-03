//! Failed observation is not a model-selection transaction rollback.
use super::*;

#[tokio::test]
async fn failed_model_publication_preserves_canonical_selection_and_version() {
    let env = namespace_environment_for_test(); // No AgentFS mount: actual write fails.
    let authority = Arc::new(GatedSelection {
        a: CapturedCallable {
            identity: CallableIdentity {
                profile: "test".into(),
                provider: "openai_responses".into(),
                model: "A".into(),
                credential_ref: None,
                revision: "A".into(),
            },
            root: env.root_transport(),
            connection: "A".into(),
            config: crate::Config::default(),
        },
        b: CapturedCallable {
            identity: CallableIdentity {
                profile: "test".into(),
                provider: "openai_responses".into(),
                model: "B".into(),
                credential_ref: None,
                revision: "B".into(),
            },
            root: env.root_transport(),
            connection: "B".into(),
            config: crate::Config::default(),
        },
        started: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Notify::new()),
    });
    authority.release.notify_one();
    let env = env.with_connection_authority(authority);
    let mut queues = RuntimeSubmissionQueues::new(Default::default());
    queues.environment = Some(env.clone());
    queues
        .initialize_bindings(&crate::Config::default(), Default::default())
        .await
        .unwrap();
    assert!(queues.publish_models().await.is_err());
    assert_eq!(queues.model_status.lock().await.publication_version, 0);
    assert!(
        queues
            .model_control(&Submission::new(Op::SelectModel { model: "B".into() }))
            .await
    );
    assert_eq!(
        env.model_bindings
            .lock()
            .await
            .confirmed
            .as_ref()
            .unwrap()
            .identity
            .model,
        "B"
    );
    assert_eq!(queues.model_status.lock().await.publication_version, 0);
    assert!(!queues.is_paused());
    let queue = queues.outer_queue.lock().unwrap();
    assert!(
        queue.admitted_ids.is_empty()
            && queue.active_submission_ids.is_empty()
            && queue.pending.is_empty()
    );
}
