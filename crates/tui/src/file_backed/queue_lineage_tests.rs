use super::*;

#[tokio::test]
async fn canonical_receipt_submitted_a_has_one_owner() {
    canonical_receipt_with_submitted_a(true).await;
}

#[tokio::test]
async fn canonical_receipt_missing_a_has_one_owner() {
    canonical_receipt_with_submitted_a(false).await;
}

async fn canonical_receipt_with_submitted_a(has_a: bool) {
    for committed in [false, true] {
        let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
        let owner = format!("/agent/{pid}");
        let body = "same body\n  界e\u{301}👩‍💻\tend";
        let mut app = FileBackedApp::new("/agent/root".into());
        app.queue.apply(
            &owner,
            Some(UiQueueSnapshot {
                known: true,
                revision: 1,
                paused: true,
                pending_submission_ids: vec!["q".into()],
                ..Default::default()
            }),
        );
        let a = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"a"});
        let answer_a = serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"answer A"});
        app.apply_tape_record(serde_json::from_value(a.clone()).unwrap());
        app.apply_tape_record(serde_json::from_value(answer_a.clone()).unwrap());
        app.track_local_input("q", owner.clone(), body.into(), InputIntent::Agent);
        app.acknowledge_local_input("q", &owner);
        let index = app.local_inputs["q"].cell.unwrap();
        app.transcript[index].trim_rendered_prefix(RenderOpts::new(80, false), 1);
        let cut = app.transcript[index].input_source().unwrap().2;
        assert_ne!(cut, (0, 0));
        app.local_inputs.get_mut("q").unwrap().source_cut = cut;
        if committed {
            app.local_inputs.get_mut("q").unwrap().committed = true;
            app.transcript[index] = HistoryCell::Styled(vec![]);
        }
        let q = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"q"});
        let answer_q = serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"answer Q"});
        let tape = if has_a {
            format!("{a}\n{answer_a}\n{q}\n{answer_q}\n")
        } else {
            format!("{q}\n{answer_q}\n")
        };
        shell
            .write(&format!("{owner}/machine/tape"), tape.as_bytes())
            .await
            .unwrap();
        shell
            .write(
                &format!("{owner}/machine/ui/queue"),
                &serde_json::to_vec(app.queue.snapshot.as_ref().unwrap()).unwrap(),
            )
            .await
            .unwrap();
        let tasks = [PendingRootAgentTurn {
            input: body.into(),
            submission_id: "a".into(),
            submitted_process: Some(pid.parse().unwrap()),
            submitted_at_ms: 0,
        }];
        for _ in 0..3 {
            let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &tasks)
                .await
                .unwrap();
            tails.close().await;
            let receipts = app
                .transcript
                .iter()
                .filter(|cell| {
                    if committed {
                        **cell == HistoryCell::Styled(vec![])
                    } else {
                        cell.input_source() == Some((body, false, cut))
                    }
                })
                .count();
            assert_eq!(receipts, 1, "canonical Q duplicated: {:?}", app.transcript);
            let qi = app.local_inputs["q"].cell.unwrap();
            assert_eq!(
                qi, 2,
                "equal-body distinct-ID A must remain: {:?}",
                app.transcript
            );
            assert_eq!(
                app.transcript[0].input_source(),
                Some((body, false, (0, 0)))
            );
            assert_eq!(app.transcript[1].assistant_source(), Some("answer A"));
            if committed {
                assert_eq!(app.transcript[qi], HistoryCell::Styled(vec![]));
            } else {
                assert_eq!(app.transcript[qi].input_source(), Some((body, false, cut)));
            }
            if has_a {
                assert_eq!(app.transcript[qi + 1].assistant_source(), Some("answer Q"));
            } else {
                assert!(
                    !app.transcript
                        .iter()
                        .any(|c| c.assistant_source() == Some("answer Q")),
                    "uncorrelated answer imported"
                );
            }
            let before = app.transcript.clone();
            app.apply_tape_record(serde_json::from_value(q.clone()).unwrap());
            assert_eq!(app.transcript, before, "late exact Tape replayed Q");
        }
    }
}

#[tokio::test]
async fn canonical_current_submitted_boundary_keeps_source() {
    remaining_canonical_scene(0).await;
}

#[tokio::test]
async fn canonical_missing_prompt_keeps_previous_answer() {
    remaining_canonical_scene(1).await;
}

#[tokio::test]
async fn canonical_multiple_missing_prompt_keeps_whole_turns() {
    remaining_canonical_scene(2).await;
}

async fn remaining_canonical_scene(scene: usize) {
    for committed in [false, true] {
        let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
        let owner = format!("/agent/{pid}");
        let body = "same body\n  界e\u{301}👩‍💻\tend";
        let mut app = FileBackedApp::new("/agent/root".into());
        let ids = if scene == 2 {
            vec!["q1", "q2"]
        } else {
            vec!["q1"]
        };
        app.queue.apply(
            &owner,
            Some(UiQueueSnapshot {
                known: true,
                revision: 1,
                paused: true,
                pending_submission_ids: ids.iter().map(|id| (*id).into()).collect(),
                ..Default::default()
            }),
        );
        if scene != 0 {
            app.transcript.push(HistoryCell::User(body.into()));
            app.transcript
                .push(HistoryCell::Assistant("answer A".into()));
        }
        let mut tape = String::new();
        let mut exact = Vec::new();
        let mut expected = app.transcript.clone();
        for id in &ids {
            app.track_local_input(id, owner.clone(), body.into(), InputIntent::Agent);
            app.acknowledge_local_input(id, &owner);
            let index = app.local_inputs[*id].cell.unwrap();
            app.transcript[index].trim_rendered_prefix(RenderOpts::new(80, false), 1);
            let cut = app.transcript[index].input_source().unwrap().2;
            assert_ne!(cut, (0, 0));
            app.local_inputs.get_mut(*id).unwrap().source_cut = cut;
            if committed {
                app.local_inputs.get_mut(*id).unwrap().committed = true;
                app.transcript[index] = HistoryCell::Styled(vec![]);
            }
            expected.push(app.transcript[index].clone());
            let answer = format!("answer {id}");
            if scene != 0 {
                app.transcript.push(HistoryCell::Assistant(answer.clone()));
            }
            expected.push(HistoryCell::Assistant(answer.clone()));
            let record = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":id});
            let hydrated_answer = if scene == 0 {
                answer
            } else {
                format!("unrelated new {id}")
            };
            let response = serde_json::json!({"version":1,"kind":"message","role":"assistant","content":hydrated_answer});
            tape.push_str(&format!("{record}\n{response}\n"));
            exact.push(record);
        }
        if scene == 2 {
            app.transcript.push(HistoryCell::User("R".into()));
            app.transcript
                .push(HistoryCell::Assistant("answer R".into()));
            expected.push(HistoryCell::User("R".into()));
            expected.push(HistoryCell::Assistant("answer R".into()));
        }
        shell
            .write(&format!("{owner}/machine/tape"), tape.as_bytes())
            .await
            .unwrap();
        shell
            .write(
                &format!("{owner}/machine/ui/queue"),
                &serde_json::to_vec(app.queue.snapshot.as_ref().unwrap()).unwrap(),
            )
            .await
            .unwrap();
        let tasks = [PendingRootAgentTurn {
            input: body.into(),
            submission_id: if scene == 0 { "q1" } else { "a" }.into(),
            submitted_process: Some(pid.parse().unwrap()),
            submitted_at_ms: 0,
        }];
        for _ in 0..3 {
            let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &tasks)
                .await
                .unwrap();
            tails.close().await;
            let actual = app
                .transcript
                .iter()
                .filter(|cell| !matches!(cell, HistoryCell::Error(_)))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "canonical scene {scene} lost source/turn chronology"
            );
            for (n, id) in ids.iter().enumerate() {
                let index = if scene == 0 { 0 } else { 2 + 2 * n };
                assert_eq!(
                    app.local_inputs[*id].cell,
                    Some(index),
                    "exact-ID anchor {id}"
                );
                assert_eq!(app.transcript[index], expected[index]);
            }
            let before = app.transcript.clone();
            for record in &exact {
                app.apply_tape_record(serde_json::from_value(record.clone()).unwrap());
            }
            assert_eq!(
                app.transcript, before,
                "late exact Tape replayed committed/source prefix"
            );
        }
    }
}

#[tokio::test]
async fn review_canonical_receipt_chronology() {
    for committed in [false, true] {
        let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
        let owner = format!("/agent/{pid}");
        let body = "same body\n  界e\u{301}👩‍💻\tend";
        let mut app = FileBackedApp::new("/agent/root".into());
        app.queue.apply(
            &owner,
            Some(UiQueueSnapshot {
                known: true,
                revision: 1,
                paused: true,
                pending_submission_ids: vec!["q".into()],
                ..Default::default()
            }),
        );
        let old = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"old"});
        app.apply_tape_record(serde_json::from_value(old.clone()).unwrap());
        app.track_local_input("q", owner.clone(), body.into(), InputIntent::Agent);
        app.acknowledge_local_input("q", &owner);
        let index = app.local_inputs["q"].cell.unwrap();
        app.transcript[index].trim_rendered_prefix(RenderOpts::new(80, false), 1);
        let cut = app.transcript[index].input_source().unwrap().2;
        app.local_inputs.get_mut("q").unwrap().source_cut = cut;
        if committed {
            app.local_inputs.get_mut("q").unwrap().committed = true;
            app.transcript[index] = HistoryCell::Styled(vec![]);
        }
        let exact = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"q"});
        let answer = serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"answer Q"});
        shell
            .write(
                &format!("{owner}/machine/tape"),
                format!("{old}\n{exact}\n{answer}\n").as_bytes(),
            )
            .await
            .unwrap();
        shell
            .write(
                &format!("{owner}/machine/ui/queue"),
                &serde_json::to_vec(app.queue.snapshot.as_ref().unwrap()).unwrap(),
            )
            .await
            .unwrap();
        for _ in 0..3 {
            let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
                .await
                .unwrap();
            tails.close().await;
            assert_eq!(app.transcript.len(), 3, "{:?}", app.transcript);
            assert_eq!(app.local_inputs["q"].cell, Some(1));
            assert_eq!(app.transcript[2].assistant_source(), Some("answer Q"));
            if committed {
                assert_eq!(app.transcript[1], HistoryCell::Styled(vec![]));
            } else {
                assert_eq!(app.transcript[1].input_source(), Some((body, false, cut)));
            }
            app.apply_tape_record(serde_json::from_value(exact.clone()).unwrap());
            assert_eq!(app.transcript.len(), 3);
        }
    }
}
use alan_agent_protocol::{InputIntent, UiQueueSnapshot};

#[tokio::test]
async fn synthetic_answer_keeps_retained_receipt_coordinates() {
    let (_, _, _, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let body = "literal Q\n  retained suffix";
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(&owner, None);
    app.track_local_input("q", owner.clone(), body.into(), InputIntent::Agent);
    app.acknowledge_local_input("q", &owner);
    let record = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"q"});
    app.apply_tape_record(serde_json::from_value(record).unwrap());
    app.transcript[0].trim_rendered_prefix(RenderOpts::new(80, false), 1);
    let cut = app.transcript[0].input_source().unwrap().2;
    assert_ne!(cut, (0, 0));
    app.local_inputs.get_mut("q").unwrap().source_cut = cut;
    for _ in 0..2 {
        previous_input::restore_answer(&mut app, body, "answer Q".into());
        assert_eq!(
            app.transcript.len(),
            2,
            "synthetic boundary duplicated receipt"
        );
        assert_eq!(app.local_inputs["q"].cell, Some(0));
        assert_eq!(app.transcript[0].input_source(), Some((body, false, cut)));
        assert_eq!(app.transcript[1].assistant_source(), Some("answer Q"));
    }
}

#[tokio::test]
async fn changed_owner_keeps_old_canonical_whole_turn() {
    use alan_agentfs::AgentFs;
    use std::sync::Arc;
    for committed in [false, true] {
        let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
        let old_owner = format!("/agent/{pid}");
        let body = "old Q\n  retained suffix";
        let mut app = FileBackedApp::new("/agent/root".into());
        app.queue.apply(&old_owner, None);
        app.transcript.push(HistoryCell::User("A".into()));
        app.transcript
            .push(HistoryCell::Assistant("answer A".into()));
        app.track_local_input("q", old_owner.clone(), body.into(), InputIntent::Agent);
        app.acknowledge_local_input("q", &old_owner);
        let record = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"q"});
        app.apply_tape_record(serde_json::from_value(record).unwrap());
        let qi = app.local_inputs["q"].cell.unwrap();
        app.transcript[qi].trim_rendered_prefix(RenderOpts::new(80, false), 1);
        let cut = app.transcript[qi].input_source().unwrap().2;
        app.local_inputs.get_mut("q").unwrap().source_cut = cut;
        if committed {
            app.local_inputs.get_mut("q").unwrap().committed = true;
            app.transcript[qi] = HistoryCell::Styled(vec![]);
        }
        app.transcript
            .push(HistoryCell::Assistant("answer Q".into()));
        let new_pid = shell.spawn(stdio_tests::EXEC_SPEC).await.unwrap();
        root.bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
            .await;
        root.set_root_process(new_pid.clone()).await;
        namespace.replace_mount(
            stdio_tests::PID_MOUNT,
            alan_ap::InProcessTransport::new(Arc::new(
                alan_ap::reference::MemFs::with_read_only_file(
                    "pid",
                    format!("{new_pid}\n").into_bytes(),
                ),
            )),
            alan_kernel::Access::ReadOnly,
        );
        let a = serde_json::json!({"version":1,"kind":"message","role":"user","content":"A","submission_id":"a"});
        shell
            .write(
                &format!("/agent/{new_pid}/machine/tape"),
                format!("{a}\n").as_bytes(),
            )
            .await
            .unwrap();
        let task = PendingRootAgentTurn {
            input: "A".into(),
            submission_id: "a".into(),
            submitted_process: Some(new_pid.parse().unwrap()),
            submitted_at_ms: 0,
        };
        for _ in 0..2 {
            let (tails, settled) = reattach_to_current_agent(
                &shell,
                "/agent/root",
                &mut app,
                std::slice::from_ref(&task),
            )
            .await
            .unwrap();
            tails.close().await;
            assert!(!settled.iter().any(|id| id == "q"));
            assert_eq!(
                app.transcript
                    .get(3)
                    .and_then(HistoryCell::assistant_source),
                Some("answer Q"),
                "old canonical turn was removed"
            );
            assert_eq!(app.local_inputs["q"].cell, Some(2));
            assert_eq!(app.local_inputs["q"].owner, old_owner);
            assert!(!app.local_inputs["q"].terminal);
            if committed {
                assert_eq!(app.transcript[2], HistoryCell::Styled(vec![]));
            } else {
                assert_eq!(app.transcript[2].input_source(), Some((body, false, cut)));
            }
            assert!(
                app.queue
                    .snapshot
                    .as_ref()
                    .is_none_or(|q| !q.pending_submission_ids.iter().any(|id| id == "q"))
            );
            assert_eq!(
                app.transcript
                    .iter()
                    .filter(|c| c.assistant_source() == Some("answer Q"))
                    .count(),
                1
            );
        }
    }
}

async fn exercise(submitted: bool, canonical: bool, equal: bool, error: bool) {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let body = "**same literal**\nretained suffix";
    let a = if equal { body } else { "A" };
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        &owner,
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: true,
            pending_submission_ids: vec!["q1".into(), "q2".into()],
            ..Default::default()
        }),
    );
    app.transcript.push(HistoryCell::User(a.into()));
    app.transcript
        .push(HistoryCell::Assistant("assistant A".into()));
    for id in ["q1", "q2"] {
        app.track_local_input(id, owner.clone(), body.into(), InputIntent::Agent);
        app.acknowledge_local_input(id, &owner);
    }
    let q1 = app.local_inputs["q1"].cell.unwrap();
    app.transcript[q1].trim_rendered_prefix(RenderOpts::new(80, false), 1);
    let cut = app.transcript[q1].input_source().unwrap().2;
    app.local_inputs.get_mut("q1").unwrap().source_cut = cut;
    if error {
        app.transcript
            .insert(0, HistoryCell::Error("old error".into()));
        for input in app.local_inputs.values_mut() {
            input.cell = input.cell.map(|i| i + 1);
        }
    }
    let mut records = Vec::new();
    if canonical {
        records.push(serde_json::json!({"version":1,"kind":"message","role":"user","content":a,"submission_id":"a"}));
        records.push(serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"assistant A extended"}));
    } else if equal {
        records.push(serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"old-u"}));
    }
    let tape = records.iter().map(|r| format!("{r}\n")).collect::<String>();
    shell
        .write(&format!("{owner}/machine/tape"), tape.as_bytes())
        .await
        .unwrap();
    shell
        .write(
            &format!("{owner}/machine/ui/queue"),
            &serde_json::to_vec(app.queue.snapshot.as_ref().unwrap()).unwrap(),
        )
        .await
        .unwrap();
    if error {
        let event = alan_agent_protocol::UiEvent::Error {
            message: "actual hydrated error".into(),
            recoverable: true,
        };
        shell
            .write(
                &format!("{owner}/machine/ui/events"),
                format!("{}\n", serde_json::to_string(&event).unwrap()).as_bytes(),
            )
            .await
            .unwrap();
    }
    let tasks = if submitted {
        vec![PendingRootAgentTurn {
            input: a.into(),
            submission_id: "a".into(),
            submitted_process: Some(pid.parse().unwrap()),
            submitted_at_ms: 0,
        }]
    } else {
        vec![]
    };
    for _ in 0..2 {
        let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &tasks)
            .await
            .unwrap();
        tails.close().await;
        for id in ["q1", "q2"] {
            let index = app.local_inputs[id].cell.expect("receipt lineage lost");
            let (text, role, retained_cut) = app.transcript[index]
                .input_source()
                .expect("receipt points at unrelated cell");
            assert_eq!(text, body);
            assert!(!role);
            assert_eq!(retained_cut, if id == "q1" { cut } else { (0, 0) });
        }
        assert_eq!(
            app.transcript
                .iter()
                .filter(|c| c.input_source().is_some_and(|(t, _, _)| t == body))
                .count(),
            if equal { 3 } else { 2 },
            "each receipt exactly once"
        );
    }
    for id in ["q1", "q2"] {
        let record = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":id});
        app.apply_tape_record(serde_json::from_value(record.clone()).unwrap());
        app.apply_tape_record(serde_json::from_value(record).unwrap());
    }
    assert!(app.transcript.iter().any(|c| {
        c.assistant_source()
            .is_some_and(|t| t.starts_with("assistant A"))
    }));
    assert_eq!(
        app.transcript
            .iter()
            .filter(|c| c.input_source().is_some_and(|(t, _, _)| t == body))
            .count(),
        if equal { 3 } else { 2 }
    );
    assert!(
        app.rendered_history_lines(80)
            .join("\n")
            .contains("**same literal**")
    );
}

#[tokio::test]
async fn review_submitted_receipt_lineage() {
    exercise(true, true, false, false).await;
}
#[tokio::test]
async fn review_submitted_interposed_error_omission() {
    exercise(true, true, false, true).await;
}
#[tokio::test]
async fn review_no_prompt_fallback() {
    exercise(true, false, false, false).await;
}
#[tokio::test]
async fn review_idle_equal_body_distinct_ids() {
    exercise(false, false, true, false).await;
}

#[tokio::test]
async fn review_idle_ambiguous_single_receipt_exact_tape() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let body = "identical full body";
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        &owner,
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: true,
            pending_submission_ids: vec!["q".into()],
            ..Default::default()
        }),
    );
    app.track_local_input("q", owner.clone(), body.into(), InputIntent::Agent);
    app.acknowledge_local_input("q", &owner);
    let old = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"old-u"});
    shell
        .write(
            &format!("{owner}/machine/tape"),
            format!("{old}\n").as_bytes(),
        )
        .await
        .unwrap();
    shell
        .write(
            &format!("{owner}/machine/ui/queue"),
            &serde_json::to_vec(app.queue.snapshot.as_ref().unwrap()).unwrap(),
        )
        .await
        .unwrap();
    for _ in 0..3 {
        let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
            .await
            .unwrap();
        tails.close().await;
        assert_eq!(app.transcript.len(), 2, "old-u and q must remain distinct");
        let index = app.local_inputs["q"].cell.unwrap();
        assert_eq!(
            app.transcript[index].input_source(),
            Some((body, false, (0, 0)))
        );
        assert!(!app.local_inputs["q"].tape_seen);
    }
    let exact = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"q"});
    for _ in 0..2 {
        app.apply_tape_record(serde_json::from_value(exact.clone()).unwrap());
    }
    assert_eq!(app.transcript.len(), 2);
    // AgentFS Tape writes append: old-u is already present, so append only q.
    shell
        .write(
            &format!("{owner}/machine/tape"),
            format!("{exact}\n").as_bytes(),
        )
        .await
        .unwrap();
    for _ in 0..2 {
        let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
            .await
            .unwrap();
        tails.close().await;
        assert_eq!(
            app.transcript.len(),
            2,
            "transcript={:?}, index={:?}",
            app.transcript,
            app.local_inputs["q"].cell
        );
        assert!(app.local_inputs["q"].tape_seen);
        assert_eq!(
            app.transcript[app.local_inputs["q"].cell.unwrap()].input_source(),
            Some((body, false, (0, 0)))
        );
    }
}

#[tokio::test]
async fn review_hydrated_error_filter_receipt_lineage() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        &owner,
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: true,
            pending_submission_ids: vec!["q".into()],
            ..Default::default()
        }),
    );
    app.track_local_input("q", owner.clone(), "hydrated Q".into(), InputIntent::Agent);
    app.acknowledge_local_input("q", &owner);
    app.transcript.clear();
    app.local_inputs.get_mut("q").unwrap().cell = None;
    app.transcript.push(HistoryCell::User("A".into()));
    let tape = serde_json::json!({"version":1,"kind":"message","role":"user","content":"A","submission_id":"a"});
    shell
        .write(
            &format!("{owner}/machine/tape"),
            format!("{tape}\n").as_bytes(),
        )
        .await
        .unwrap();
    let event = alan_agent_protocol::UiEvent::Error {
        message: "hydrated failure".into(),
        recoverable: true,
    };
    shell
        .write(
            &format!("{owner}/machine/ui/events"),
            format!("{}\n", serde_json::to_string(&event).unwrap()).as_bytes(),
        )
        .await
        .unwrap();
    shell
        .write(
            &format!("{owner}/machine/ui/queue"),
            &serde_json::to_vec(app.queue.snapshot.as_ref().unwrap()).unwrap(),
        )
        .await
        .unwrap();
    let task = PendingRootAgentTurn {
        input: "A".into(),
        submission_id: "a".into(),
        submitted_process: Some(pid.parse().unwrap()),
        submitted_at_ms: 0,
    };
    for _ in 0..2 {
        let (tails, _) =
            reattach_to_current_agent(&shell, "/agent/root", &mut app, std::slice::from_ref(&task))
                .await
                .unwrap();
        tails.close().await;
        assert_eq!(
            app.transcript[app.local_inputs["q"].cell.unwrap()].input_source(),
            Some(("hydrated Q", false, (0, 0)))
        );
        assert_eq!(
            app.transcript
                .iter()
                .filter(|c| c.input_source().is_some_and(|(t, _, _)| t == "hydrated Q"))
                .count(),
            1
        );
    }
}
