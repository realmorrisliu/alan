#[tokio::test]
async fn connection_models_file_falls_back_to_provider_baseline_and_updates_live_via_ap() {
    let fs = LlmFs::new();
    fs.register_connection_profile(
        "vertex-main",
        ConnectionProfile::new(
            "google_gemini_generate_content",
            "gemini-2.5-flash",
            "vertex-adc",
        ),
        Box::new(MockLlmProvider::new()),
    );
    fs.register_connection_alias("default", "vertex-main")
        .unwrap();

    // `models` is listed under `connections/<conn>/` and is read-only.
    let listing =
        String::from_utf8(read_all(&fs, &["connections", "vertex-main"], Fid(200)).await).unwrap();
    assert!(
        listing.lines().any(|line| line == "models"),
        "connection dir should list models: {listing:?}"
    );
    fs.walk(
        Fid::ROOT,
        Fid(201),
        &[
            "connections".into(),
            "vertex-main".into(),
            "models".into(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(fs.open(Fid(201), OpenMode::Write).await, Err(ErrorCode::NoAccess));
    let initial_stat = fs.stat(Fid(201)).await.unwrap();

    // Before live catalog publication, falls back to canonical provider baseline.
    let baseline_doc: serde_json::Value = serde_json::from_slice(
        &read_all(&fs, &["connections", "vertex-main", "models"], Fid(202)).await,
    )
    .unwrap();
    assert_eq!(baseline_doc["version"], 1);
    assert_eq!(baseline_doc["connection"], "vertex-main");
    assert_eq!(baseline_doc["provider"], "google_gemini_generate_content");
    assert_eq!(baseline_doc["source"], "bundled-gemini");
    let baseline_slugs: Vec<&str> = baseline_doc["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["slug"].as_str())
        .collect();
    let expected_baseline: Vec<String> = alan_llm::gemini_catalog::vertex_baseline_catalog()
        .into_iter()
        .map(|m| m.slug)
        .collect();
    assert_eq!(
        baseline_slugs,
        expected_baseline
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );

    // `/providers/google_gemini_generate_content/models` uses the exact same single baseline.
    let provider_doc: serde_json::Value = serde_json::from_slice(
        &read_all(
            &fs,
            &["providers", "google_gemini_generate_content", "models"],
            Fid(203),
        )
        .await,
    )
    .unwrap();
    let provider_slugs: Vec<&str> = provider_doc["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["slug"].as_str())
        .collect();
    assert_eq!(provider_slugs, baseline_slugs);

    // Publish live models; both the named connection and its `default` alias update and bump qid version.
    let live_models = vec![
        alan_llm::ProviderModel {
            slug: "gemini-4.0-pro-preview".to_string(),
            context_window_tokens: 1_048_576,
            supported_reasoning_efforts: vec![
                alan_llm::ReasoningEffort::Low,
                alan_llm::ReasoningEffort::Medium,
                alan_llm::ReasoningEffort::High,
            ],
            default_reasoning_effort: Some(alan_llm::ReasoningEffort::High),
        },
        alan_llm::ProviderModel {
            slug: "gemini-2.5-flash".to_string(),
            context_window_tokens: 1_048_576,
            supported_reasoning_efforts: vec![
                alan_llm::ReasoningEffort::None,
                alan_llm::ReasoningEffort::Low,
                alan_llm::ReasoningEffort::Medium,
                alan_llm::ReasoningEffort::High,
            ],
            default_reasoning_effort: Some(alan_llm::ReasoningEffort::Medium),
        },
    ];
    assert!(fs.set_connection_models("vertex-main", live_models.clone()));
    assert!(!fs.set_connection_models("missing", live_models));

    let updated_stat = fs.stat(Fid(201)).await.unwrap();
    assert!(
        updated_stat.qid.version > initial_stat.qid.version,
        "publishing live models must bump models qid version"
    );

    for conn in ["vertex-main", "default"] {
        let live_doc: serde_json::Value = serde_json::from_slice(
            &read_all(&fs, &["connections", conn, "models"], Fid(204)).await,
        )
        .unwrap();
        fs.clunk(Fid(204)).await.unwrap();
        assert_eq!(live_doc["version"], 1);
        assert_eq!(live_doc["connection"], conn);
        assert_eq!(live_doc["provider"], "google_gemini_generate_content");
        assert_eq!(live_doc["source"], "live");
        assert_eq!(live_doc["models"][0]["slug"], "gemini-4.0-pro-preview");
        assert_eq!(live_doc["models"][0]["context_window_tokens"], 1_048_576);
        assert_eq!(live_doc["models"][0]["supports_reasoning"], true);
        assert_eq!(
            live_doc["models"][0]["supported_reasoning_efforts"],
            serde_json::json!(["low", "medium", "high"])
        );
        assert_eq!(live_doc["models"][0]["default_reasoning_effort"], "high");
    }

    // Narrowed views and snapshots inherit the live model catalog cleanly.
    let snapshot = fs.connection_snapshot("vertex-main");
    let snap_doc: serde_json::Value = serde_json::from_slice(
        &read_all(&snapshot, &["connections", "vertex-main", "models"], Fid(205)).await,
    )
    .unwrap();
    assert_eq!(snap_doc["source"], "live");
    assert_eq!(snap_doc["models"][0]["slug"], "gemini-4.0-pro-preview");

    // Clearing live models falls back cleanly to provider baseline again.
    assert!(fs.clear_connection_models("vertex-main"));
    let cleared_doc: serde_json::Value = serde_json::from_slice(
        &read_all(&fs, &["connections", "vertex-main", "models"], Fid(206)).await,
    )
    .unwrap();
    assert_eq!(cleared_doc["source"], "bundled-gemini");
}
