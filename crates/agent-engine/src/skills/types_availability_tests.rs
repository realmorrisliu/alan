use super::*;

#[test]
fn test_skill_availability_tracks_tools_and_min_version() {
    let metadata = SkillMetadata {
        id: "test-skill".to_string(),
        package_id: Some("skill:test-skill".to_string()),
        name: "Test Skill".to_string(),
        description: "A test skill".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/test-skill/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: Some(SkillCapabilities {
            required_tools: vec!["read_file".to_string()],
            ..Default::default()
        }),
        compatibility: SkillCompatibility {
            min_version: Some("0.2.0".to_string()),
            dependencies: Vec::new(),
            requirements: None,
        },
        source: SkillContentSource::File(PathBuf::from("/tmp/test-skill/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    };

    let unavailable = skill_availability_issues(
        &metadata,
        &SkillHostCapabilities::default().with_runtime_defaults(),
    );
    assert_eq!(unavailable.len(), 2);
    assert!(!is_skill_available(
        &metadata,
        &SkillHostCapabilities::default().with_runtime_defaults()
    ));

    let available_host = SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults();
    let issues = skill_availability_issues(
        &SkillMetadata {
            compatibility: SkillCompatibility {
                min_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                ..metadata.compatibility.clone()
            },
            ..metadata
        },
        &available_host,
    );
    assert!(issues.is_empty());
}

#[test]
fn test_skill_availability_accepts_host_executable_dependencies() {
    let metadata = SkillMetadata {
        id: "jq-summary".to_string(),
        package_id: Some("skill:jq-summary".to_string()),
        name: "JQ Summary".to_string(),
        description: "Summarize JSON with jq".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/jq-summary/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: Some(SkillCapabilities {
            required_tools: vec!["jq".to_string()],
            ..Default::default()
        }),
        compatibility: Default::default(),
        source: SkillContentSource::File(PathBuf::from("/tmp/jq-summary/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    };

    let missing = skill_availability_issues(
        &metadata,
        &SkillHostCapabilities::default().with_runtime_defaults(),
    );
    assert_eq!(
        missing,
        vec![SkillAvailabilityIssue::MissingDependencies(vec![
            SkillDependencyIssue::MissingTool {
                name: "jq".to_string(),
                description: None,
            }
        ])]
    );

    let available = skill_availability_issues(
        &metadata,
        &SkillHostCapabilities::default()
            .with_executables(["jq"])
            .with_runtime_defaults(),
    );
    assert!(available.is_empty());
}

#[test]
fn test_skill_remediation_suggests_next_steps() {
    let metadata = SkillMetadata {
        id: "test-skill".to_string(),
        package_id: Some("skill:test-skill".to_string()),
        name: "Test Skill".to_string(),
        description: "A test skill".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/test-skill/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: Some(SkillCapabilities {
            required_tools: vec!["read_file".to_string(), "bash".to_string()],
            ..Default::default()
        }),
        compatibility: SkillCompatibility {
            min_version: Some("9.9.9".to_string()),
            dependencies: Vec::new(),
            requirements: Some("needs local Docker access".to_string()),
        },
        source: SkillContentSource::File(PathBuf::from("/tmp/test-skill/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    };

    let remediation = skill_remediation(
        &metadata,
        &SkillHostCapabilities::default().with_runtime_defaults(),
    )
    .unwrap();

    assert!(
        remediation
            .reasons
            .iter()
            .any(|reason| reason.contains("missing dependencies:"))
    );
    assert!(
        remediation
            .next_steps
            .iter()
            .any(|step| step.contains("Enable or register the required tool:"))
    );
    assert!(
        remediation
            .next_steps
            .iter()
            .any(|step| step.contains("Upgrade alan"))
    );
    assert!(
        remediation
            .next_steps
            .iter()
            .any(|step| step.contains("needs local Docker access"))
    );
}

#[test]
fn test_delegated_invocation_is_not_a_runtime_default_tool() {
    let metadata = SkillMetadata {
        id: "repo-review".to_string(),
        package_id: Some("skill:repo-review".to_string()),
        name: "Repo Review".to_string(),
        description: "Delegated repository review".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/repo-review/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: Some(SkillCapabilities {
            required_tools: vec!["invoke_delegated_skill".to_string()],
            ..Default::default()
        }),
        compatibility: Default::default(),
        source: SkillContentSource::File(PathBuf::from("/tmp/repo-review/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    };

    let default_runtime = SkillHostCapabilities::default().with_runtime_defaults();
    let issues = skill_availability_issues(&metadata, &default_runtime);
    assert_eq!(
        issues,
        vec![SkillAvailabilityIssue::MissingDependencies(vec![
            SkillDependencyIssue::MissingTool {
                name: "invoke_delegated_skill".to_string(),
                description: None,
            }
        ])]
    );

    let delegated_runtime = SkillHostCapabilities::default()
        .with_runtime_defaults()
        .with_delegated_skill_invocation();
    assert!(skill_availability_issues(&metadata, &delegated_runtime).is_empty());
}

#[test]
fn test_unresolved_execution_is_reported_as_unavailable() {
    let metadata = SkillMetadata {
        id: "skill-creator".to_string(),
        package_id: Some("skill:skill-creator".to_string()),
        name: "Skill Creator".to_string(),
        description: "Creates new skills".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/skill-creator/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(PathBuf::from("/tmp/skill-creator/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: ResolvedSkillExecution::Unresolved {
            reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                skill_id: "skill-creator".to_string(),
                child_agent_exports: vec!["creator".to_string(), "grader".to_string()],
            },
        },
    };

    let issues = skill_availability_issues(
        &metadata,
        &SkillHostCapabilities::default().with_runtime_defaults(),
    );
    assert_eq!(
        issues,
        vec![SkillAvailabilityIssue::UnresolvedExecution(
            "unresolved(ambiguous_package_shape)".to_string(),
        )]
    );

    let remediation =
        skill_remediation_from_issues(&metadata, &issues).expect("expected remediation");
    assert!(
        remediation
            .next_steps
            .iter()
            .any(|step| step.contains("Fix delegated execution metadata"))
    );
}

#[test]
fn test_typed_env_var_dependencies_drive_availability_and_remediation() {
    let metadata = SkillMetadata {
        id: "openai-docs".to_string(),
        package_id: Some("skill:openai-docs".to_string()),
        name: "OpenAI Docs".to_string(),
        description: "Use official OpenAI docs".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/openai-docs/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: None,
        compatibility: SkillCompatibility {
            min_version: None,
            dependencies: vec![SkillTypedDependency::EnvVar {
                name: "OPENAI_API_KEY".to_string(),
                description: Some("Required API key".to_string()),
            }],
            requirements: None,
        },
        source: SkillContentSource::File(PathBuf::from("/tmp/openai-docs/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    };

    let missing_issues = skill_availability_issues(
        &metadata,
        &SkillHostCapabilities::default().with_runtime_defaults(),
    );
    assert_eq!(
        missing_issues,
        vec![SkillAvailabilityIssue::MissingDependencies(vec![
            SkillDependencyIssue::MissingEnvVar {
                name: "OPENAI_API_KEY".to_string(),
                description: Some("Required API key".to_string()),
            }
        ])]
    );

    let remediation =
        skill_remediation_from_issues(&metadata, &missing_issues).expect("remediation");
    assert!(
        remediation
            .next_steps
            .iter()
            .any(|step| step.contains("Set the required environment variable: OPENAI_API_KEY."))
    );

    let available_host = SkillHostCapabilities::default()
        .with_env_vars(["OPENAI_API_KEY"])
        .with_runtime_defaults();
    assert!(skill_availability_issues(&metadata, &available_host).is_empty());
}

#[test]
fn test_process_env_ignores_empty_env_var_values() {
    let mut capabilities = SkillHostCapabilities::default();
    capabilities.extend_env_var_values([
        ("OPENAI_API_KEY".to_string(), "".to_string()),
        ("ANTHROPIC_API_KEY".to_string(), "sk-ant-123".to_string()),
    ]);

    assert!(!capabilities.supports_env_var("OPENAI_API_KEY"));
    assert!(capabilities.supports_env_var("ANTHROPIC_API_KEY"));
}

#[test]
fn test_normalize_env_var_name_supports_case_insensitive_hosts() {
    assert_eq!(
        normalize_env_var_name("openai_api_key", true),
        "OPENAI_API_KEY"
    );
    assert_eq!(
        normalize_env_var_name("OpenAi_Api_Key", true),
        "OPENAI_API_KEY"
    );
    assert_eq!(
        normalize_env_var_name("OpenAi_Api_Key", false),
        "OpenAi_Api_Key"
    );
}

#[test]
fn test_compatibility_metadata_dependency_hints_do_not_gate_runtime_availability() {
    let metadata = SkillMetadata {
        id: "openai-docs".to_string(),
        package_id: Some("skill:openai-docs".to_string()),
        name: "OpenAI Docs".to_string(),
        description: "Use official OpenAI docs".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/openai-docs/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(PathBuf::from("/tmp/openai-docs/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: CompatibleSkillMetadata {
            interface: Default::default(),
            dependencies: CompatibleSkillDependencies {
                tools: vec![
                    CompatibleSkillToolDependency {
                        kind: Some("env".to_string()),
                        value: Some("OPENAI_API_KEY".to_string()),
                        description: Some("Required API key".to_string()),
                        transport: None,
                        command: None,
                        url: None,
                    },
                    CompatibleSkillToolDependency {
                        kind: Some("mcp".to_string()),
                        value: Some("openaiDeveloperDocs".to_string()),
                        description: Some("OpenAI Developer Docs MCP server".to_string()),
                        transport: Some("streamable_http".to_string()),
                        command: None,
                        url: Some("https://developers.openai.com/mcp".to_string()),
                    },
                ],
            },
            policy: Default::default(),
        },
        execution: Default::default(),
    };

    let issues = skill_availability_issues(
        &metadata,
        &SkillHostCapabilities::default().with_runtime_defaults(),
    );
    assert!(issues.is_empty());
}

#[test]
fn test_typed_tool_dependencies_reject_blank_names() {
    let compatibility = SkillCompatibility {
        min_version: None,
        dependencies: vec![SkillTypedDependency::Tool {
            name: "".to_string(),
            description: Some("Broken dependency".to_string()),
        }],
        requirements: None,
    };

    let err = validate_skill_compatibility(&compatibility).expect_err("expected invalid tool");
    assert!(
        matches!(err, SkillsError::InvalidCapabilities(message) if message.contains("Invalid tool name"))
    );
}

#[test]
fn test_required_tools_reject_whitespace_only_names() {
    let capabilities = SkillCapabilities {
        required_tools: vec!["\t".to_string()],
        ..Default::default()
    };

    let err = validate_capabilities(&capabilities).expect_err("expected invalid tool");
    assert!(
        matches!(err, SkillsError::InvalidCapabilities(message) if message.contains("Invalid tool name"))
    );
}

#[test]
fn test_skill_host_capabilities_runtime_defaults_include_virtual_tools() {
    let capabilities = SkillHostCapabilities::default().with_runtime_defaults();
    assert!(capabilities.tools.contains("request_confirmation"));
    assert!(capabilities.tools.contains("request_mount"));
    assert!(capabilities.tools.contains("request_user_input"));
    assert!(capabilities.tools.contains("update_plan"));
    assert!(!capabilities.tools.contains("invoke_delegated_skill"));
    assert!(!capabilities.supports_delegated_skill_invocation());

    let delegated_capabilities = SkillHostCapabilities::default()
        .with_runtime_defaults()
        .with_delegated_skill_invocation();
    assert!(
        delegated_capabilities
            .tools
            .contains("invoke_delegated_skill")
    );
    assert!(delegated_capabilities.supports_delegated_skill_invocation());
}

#[test]
fn test_required_tool_support_does_not_treat_dynamic_name_match_as_delegated_runtime_support() {
    let mut capabilities = SkillHostCapabilities::default().with_runtime_defaults();
    capabilities.extend_tools(["invoke_delegated_skill"]);

    assert!(capabilities.tools.contains("invoke_delegated_skill"));
    assert!(!capabilities.supports_delegated_skill_invocation());
    assert!(!capabilities.supports_required_tool("invoke_delegated_skill"));
}

#[test]
fn test_request_mount_required_tool_is_runtime_backed() {
    let capabilities = SkillHostCapabilities::default().with_runtime_defaults();

    assert!(capabilities.supports_required_tool("request_mount"));

    let executable_only = SkillHostCapabilities::default().with_executables(["request_mount"]);
    assert!(!executable_only.supports_required_tool("request_mount"));
}

#[test]
fn test_process_path_executables_collect_host_commands() {
    let temp = tempfile::tempdir().unwrap();
    let executable_path = {
        #[cfg(windows)]
        {
            temp.path().join("demo.cmd")
        }

        #[cfg(not(windows))]
        {
            temp.path().join("demo")
        }
    };
    std::fs::write(&executable_path, "echo demo\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(&executable_path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable_path, permissions).unwrap();
    }

    let mut capabilities = SkillHostCapabilities::default();
    capabilities.extend_executables_from_path_dirs([temp.path()]);

    assert!(capabilities.supports_required_tool("demo"));
}

#[test]
fn test_executable_name_normalization_supports_case_insensitive_hosts() {
    assert_eq!(normalize_executable_name("JQ", true), "jq");
    assert_eq!(normalize_executable_name("jq", false), "jq");

    let capabilities = SkillHostCapabilities::default().with_executables(["JQ"]);
    assert!(
        capabilities
            .executables
            .contains(&normalize_executable_name_for_host("JQ"))
    );

    if cfg!(windows) {
        assert!(capabilities.supports_required_tool("jq"));
    } else {
        assert!(!capabilities.supports_required_tool("jq"));
    }
}

#[test]
fn test_required_runtime_tool_is_not_satisfied_by_path_executable() {
    let capabilities = SkillHostCapabilities::default().with_executables(["bash"]);

    assert!(!capabilities.supports_required_tool("bash"));
}

#[test]
fn test_delegated_skill_result_serializes_minimal_bounded_payload() {
    let result = DelegatedSkillResult::completed(
        "Delegated child finished review.",
        Some(serde_json::json!({
            "score": "pass"
        })),
    );

    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["status"], "completed");
    assert_eq!(value["summary"], "Delegated child finished review.");
    assert_eq!(value["structured_output"]["score"], "pass");
}

#[test]
fn test_delegated_skill_invocation_record_captures_task_and_result() {
    let record = DelegatedSkillInvocationRecord {
        skill_id: "repo-review".to_string(),
        target: "repo-review".to_string(),
        task: "Review the current diff.".to_string(),
        cwd: Some("/mnt/source/src".to_string()),
        timeout_secs: Some(600),
        result: DelegatedSkillResult::failed(
            "Child-agent spawn support is not yet available.",
            Some(serde_json::json!({
                "error_kind": "runtime_child_launch_unavailable"
            })),
        ),
    };

    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["skill_id"], "repo-review");
    assert_eq!(value["target"], "repo-review");
    assert_eq!(value["cwd"], "/mnt/source/src");
    assert_eq!(value["timeout_secs"], 600);
    assert_eq!(value["task"], "Review the current diff.");
    assert_eq!(value["result"]["status"], "failed");
    assert_eq!(
        value["result"]["structured_output"]["error_kind"],
        "runtime_child_launch_unavailable"
    );
}

#[test]
fn test_capability_child_agent_export_builds_package_handle() {
    let handle = CapabilityChildAgentExport::package_handle("skill:repo-review", "reviewer");
    assert_eq!(
        handle,
        alan_agent_protocol::SpawnTarget::PackageChildAgent {
            package_id: "skill:repo-review".to_string(),
            export_name: "reviewer".to_string(),
        }
    );
}

#[test]
fn test_skill_metadata_delegated_spawn_target_uses_package_handle() {
    let metadata = SkillMetadata {
        id: "repo-review".to_string(),
        package_id: Some("skill:repo-review".to_string()),
        name: "Repo Review".to_string(),
        description: "Review repository changes".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/repo-review/SKILL.md"),
        package_root: Some(PathBuf::from("/tmp/repo-review")),
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: Vec::new(),
        capabilities: None,
        compatibility: SkillCompatibility::default(),
        source: SkillContentSource::File(PathBuf::from("/tmp/repo-review/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: AlanSkillRuntimeMetadata::default(),
        compatible_metadata: Default::default(),
        execution: ResolvedSkillExecution::Delegate {
            target: "reviewer".to_string(),
            source: SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
        },
    };

    assert_eq!(
        metadata.delegated_spawn_target(),
        Some(alan_agent_protocol::SpawnTarget::PackageChildAgent {
            package_id: "skill:repo-review".to_string(),
            export_name: "reviewer".to_string(),
        })
    );
}

#[test]
fn test_skill_availability_respects_semver_prerelease_ordering() {
    let metadata = SkillMetadata {
        id: "test-skill".to_string(),
        package_id: Some("skill:test-skill".to_string()),
        name: "Test Skill".to_string(),
        description: "A test skill".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/test-skill/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: None,
        compatibility: SkillCompatibility {
            min_version: Some("1.2.3".to_string()),
            dependencies: Vec::new(),
            requirements: None,
        },
        source: SkillContentSource::File(PathBuf::from("/tmp/test-skill/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    };
    let host_capabilities = SkillHostCapabilities {
        alan_version: "1.2.3-alpha.1".to_string(),
        ..SkillHostCapabilities::default()
    }
    .with_runtime_defaults();

    let issues = skill_availability_issues(&metadata, &host_capabilities);
    assert_eq!(
        issues,
        vec![SkillAvailabilityIssue::MinVersionNotMet {
            required: "1.2.3".to_string(),
            current: "1.2.3-alpha.1".to_string(),
        }]
    );
}

#[test]
fn test_skill_availability_accepts_semver_build_metadata() {
    let metadata = SkillMetadata {
        id: "test-skill".to_string(),
        package_id: Some("skill:test-skill".to_string()),
        name: "Test Skill".to_string(),
        description: "A test skill".to_string(),
        short_description: None,
        path: PathBuf::from("/tmp/test-skill/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: None,
        compatibility: SkillCompatibility {
            min_version: Some("1.2.3+build.5".to_string()),
            dependencies: Vec::new(),
            requirements: None,
        },
        source: SkillContentSource::File(PathBuf::from("/tmp/test-skill/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    };
    let host_capabilities = SkillHostCapabilities {
        alan_version: "1.2.3".to_string(),
        ..SkillHostCapabilities::default()
    }
    .with_runtime_defaults();

    let issues = skill_availability_issues(&metadata, &host_capabilities);
    assert!(issues.is_empty());
}
