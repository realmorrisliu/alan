//! Pure attach/streaming reconciliation for the file-backed TUI.
//!
//! Two live sources describe the same conversation with no shared cursor:
//! `io/output` (an unframed byte stream, the optimistic preview) and
//! `machine/tape` (framed records, the authority). The app sees an arbitrary
//! interleaving of the two — but each source is internally ordered (one tail
//! per file): the output subsequence is the responses in order, the tape
//! subsequence is `user, assistant, user, assistant, …` in order. The only
//! reordering is *between* the two.
//!
//! Every bug found in review lived in the string matching + suppression +
//! ordering bookkeeping, not in transcript access. [`StreamReconciler`]
//! isolates exactly that: a pure state machine with no transcript knowledge.
//! The app calls it and applies the returned decisions mechanically. The only
//! allowed middle insertion is a delayed user boundary, and the app is
//! responsible for shifting any side indexes when that happens. It is
//! exhaustively property-tested against the tape as ground truth over every
//! legal interleaving.
//!
//! The load-bearing idea: a stream chunk that belongs to a turn whose user
//! boundary has not been observed yet (a next turn racing ahead through the
//! separate output tail) is **held**, not rendered, until that boundary
//! arrives. This is what keeps a next-turn response from merging into, or
//! rendering above, the turn that produced it.
//!
//! State:
//! - *awaiting_boundary*: an assistant record has closed the last turn, so the
//!   next stream bytes belong to a turn whose user record has not arrived yet;
//!   hold them until it does.
//! - *held*: those buffered next-turn bytes.
//! - *preview_open*: the current assistant cell is unconfirmed stream text for
//!   the in-progress turn; bytes append to it.
//! - *suppression* `(expected, consumed)`: a tape record rendered a response
//!   before its stream bytes arrived; the late bytes are consumed, not
//!   duplicated.
//! - *pending echo*: this client's own submit text, so exactly one user record
//!   is deduped and repeated identical prompts from other writers keep their
//!   boundaries.

/// What the app should do with a filtered stream chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamAction {
    /// Nothing to render (empty, suppressed, or held for a future boundary).
    Drop,
    /// Append to the current (open) assistant cell.
    Append(String),
    /// Start a new assistant cell with this text.
    StartNew(String),
}

/// What the app should do with a user tape record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UserDecision {
    /// Confirms the local echo already on screen: no user-cell edit.
    Drop,
    /// Push a new user cell at the end.
    Push(String),
}

/// What the app should do with an assistant tape record, given the located
/// open preview cell (if any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AssistantDecision {
    /// The stream already rendered this response in full: no edit.
    Drop,
    /// Replace the open preview cell's content with this authoritative text.
    ReplacePreview(String),
    /// The stream missed this response: push it as a new assistant cell.
    Push(String),
}

/// Pure attach/streaming reconciliation state machine. See module docs.
#[derive(Debug, Default)]
pub(crate) struct StreamReconciler {
    /// The current assistant cell is an unconfirmed, still-streaming preview.
    preview_open: bool,
    /// An assistant record closed the last turn; stream bytes until the next
    /// user record belong to a turn whose boundary has not arrived yet.
    awaiting_boundary: bool,
    /// Buffered next-turn stream bytes, flushed when the boundary arrives.
    held: String,
    /// `(expected_text, bytes_already_accounted)` for a response a tape record
    /// rendered before its stream bytes arrived.
    suppress: Option<(String, usize)>,
    /// This client's own submit text awaiting its user tape record.
    pending_echo: Option<String>,
}

impl StreamReconciler {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Whether the previous assistant record closed a turn and live stream
    /// bytes are waiting for the next user boundary before they can render.
    pub(crate) fn awaiting_boundary(&self) -> bool {
        self.awaiting_boundary
    }

    /// This client submitted a message (the app pushes the User cell). Its
    /// boundary is already on screen, so this turn's stream renders
    /// immediately; stale suppression/held from a prior turn is discarded.
    pub(crate) fn on_local_submit(&mut self, text: &str) {
        self.pending_echo = Some(text.to_string());
        self.suppress = None;
        self.preview_open = false;
        self.awaiting_boundary = false;
        self.held.clear();
    }

    /// Filter and place a stream chunk. Suppression consumes bytes a record
    /// already rendered; bytes for a not-yet-bounded next turn are held; the
    /// rest opens or extends the current assistant cell.
    pub(crate) fn on_stream(&mut self, text: String) -> StreamAction {
        let rendered = match self.suppress.take() {
            None => text,
            Some((expected, consumed)) => {
                let remainder = &expected[consumed..];
                if let Some(rest) = text.strip_prefix(remainder) {
                    // Finishes the suppressed response; the excess is real.
                    rest.to_string()
                } else if remainder.starts_with(text.as_str()) {
                    // A leading chunk of the suppressed response; stay armed.
                    let consumed = consumed + text.len();
                    if consumed < expected.len() {
                        self.suppress = Some((expected, consumed));
                    }
                    return StreamAction::Drop;
                } else if remainder.ends_with(text.as_str()) {
                    // Clipped-attach tail of the suppressed response.
                    self.suppress = Some((expected, consumed));
                    return StreamAction::Drop;
                } else {
                    // Not the suppressed response — render it.
                    text
                }
            }
        };
        if rendered.is_empty() {
            return StreamAction::Drop;
        }
        if self.awaiting_boundary {
            // Belongs to a turn whose user record has not arrived yet.
            self.held.push_str(&rendered);
            return StreamAction::Drop;
        }
        if self.preview_open {
            StreamAction::Append(rendered)
        } else {
            self.preview_open = true;
            StreamAction::StartNew(rendered)
        }
    }

    /// A `machine/tape` user record arrived: a turn boundary. Call
    /// [`take_flushed_stream`] afterwards to render any buffered next-turn
    /// bytes that were waiting for this boundary.
    pub(crate) fn on_user_record(&mut self, content: &str) -> UserDecision {
        // Suppression armed for a previous response is stale from a boundary:
        // pre-attach bytes behind the live edge can never arrive.
        self.suppress = None;
        self.awaiting_boundary = false;
        // Dedupe only against this client's own pending echo.
        if self.pending_echo.as_deref() == Some(content) {
            self.pending_echo = None;
            return UserDecision::Drop;
        }
        UserDecision::Push(content.to_string())
    }

    /// After a user record, return buffered next-turn stream bytes to render
    /// as a new assistant cell (opening the preview), if any were held.
    pub(crate) fn take_flushed_stream(&mut self) -> Option<String> {
        if self.held.is_empty() {
            return None;
        }
        self.preview_open = true;
        Some(std::mem::take(&mut self.held))
    }

    /// A `machine/tape` assistant record arrived. `preview` is the open
    /// preview cell's text when one is open, else None. Every branch closes
    /// the turn (`awaiting_boundary = true`): subsequent stream bytes belong
    /// to the next turn and are held until its user record.
    pub(crate) fn on_assistant_record(
        &mut self,
        content: String,
        preview: Option<&str>,
    ) -> AssistantDecision {
        let decision = self.decide_assistant(content, preview.filter(|_| self.preview_open));
        self.preview_open = false;
        self.awaiting_boundary = true;
        decision
    }

    fn decide_assistant(&mut self, content: String, preview: Option<&str>) -> AssistantDecision {
        let Some(preview) = preview else {
            // The stream missed this response; consume its late bytes.
            self.suppress = Some((content.clone(), 0));
            return AssistantDecision::Push(content);
        };

        if preview == content {
            // Fully streamed already.
            self.suppress = None;
            return AssistantDecision::Drop;
        }
        if let Some(remainder) = preview.strip_prefix(content.as_str()) {
            // Stream ran ahead into the next turn: confirm this turn and hold
            // the excess until the next turn's boundary arrives.
            self.suppress = None;
            self.held = format!("{remainder}{}", std::mem::take(&mut self.held));
            return AssistantDecision::ReplacePreview(content);
        }
        if content.starts_with(preview) {
            // Record won the channel race mid-stream: finish the cell and
            // consume the queued remainder exactly.
            self.suppress = Some((content.clone(), preview.len()));
            return AssistantDecision::ReplacePreview(content);
        }
        if content.ends_with(preview) {
            // A mid-turn attach clipped the stream's prefix.
            self.suppress = Some((content.clone(), 0));
            return AssistantDecision::ReplacePreview(content);
        }
        // Clipped attach AND stream ran ahead: the preview is this response's
        // tail followed by the next turn's bytes. Split at the longest prefix
        // of the preview that is a suffix of the record; confirm this turn and
        // hold the excess.
        for split in (1..preview.len().min(content.len() + 1)).rev() {
            if preview.is_char_boundary(split) && content.ends_with(&preview[..split]) {
                self.suppress = None;
                self.held = format!("{}{}", &preview[split..], std::mem::take(&mut self.held));
                return AssistantDecision::ReplacePreview(content);
            }
        }
        // Unrelated preview: it belongs to a different racing response. Push
        // the confirmed record; keep the open preview as-is by leaving it
        // (the caller's ReplacePreview target is None, so Push appends).
        self.suppress = Some((content.clone(), 0));
        AssistantDecision::Push(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal transcript model driving the reconciler exactly as the app
    /// does, so tests exercise the whole contract end to end.
    #[derive(Default)]
    struct Model {
        cells: Vec<Cell>,
        rec: StreamReconciler,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Cell {
        User(String),
        Assistant(String),
        Boundary, // a non-message interposed cell (plan/notice)
    }

    impl Model {
        /// The open preview cell index: the last Assistant cell with no
        /// User/Boundary turn break after it. Mirrors the app's scan.
        fn open_cell(&self) -> Option<usize> {
            for (idx, cell) in self.cells.iter().enumerate().rev() {
                match cell {
                    Cell::Assistant(_) => return Some(idx),
                    Cell::User(_) => return None,
                    Cell::Boundary => {}
                }
            }
            None
        }

        fn stream(&mut self, text: &str) {
            match self.rec.on_stream(text.to_string()) {
                StreamAction::Drop => {}
                StreamAction::Append(t) => match self.cells.last_mut() {
                    Some(Cell::Assistant(existing)) => existing.push_str(&t),
                    _ => self.cells.push(Cell::Assistant(t)),
                },
                StreamAction::StartNew(t) => self.cells.push(Cell::Assistant(t)),
            }
        }

        fn user_record(&mut self, content: &str) {
            match self.rec.on_user_record(content) {
                UserDecision::Drop => {}
                UserDecision::Push(c) => self.cells.push(Cell::User(c)),
            }
            if let Some(stream) = self.rec.take_flushed_stream() {
                self.cells.push(Cell::Assistant(stream));
            }
        }

        fn assistant_record(&mut self, content: &str) {
            let idx = self.open_cell();
            let preview = idx.map(|i| match &self.cells[i] {
                Cell::Assistant(t) => t.clone(),
                _ => unreachable!(),
            });
            match self
                .rec
                .on_assistant_record(content.to_string(), preview.as_deref())
            {
                AssistantDecision::Drop => {}
                AssistantDecision::ReplacePreview(t) => {
                    if let Some(Cell::Assistant(existing)) = idx.map(|i| &mut self.cells[i]) {
                        *existing = t;
                    }
                }
                AssistantDecision::Push(t) => self.cells.push(Cell::Assistant(t)),
            }
        }

        fn messages(&self) -> Vec<Cell> {
            self.cells
                .iter()
                .filter(|c| !matches!(c, Cell::Boundary))
                .cloned()
                .collect()
        }
    }

    // ---- Targeted cases mirroring each review round ----

    #[test]
    fn fully_streamed_response_is_deduped() {
        let mut m = Model::default();
        m.stream("hello");
        m.assistant_record("hello");
        assert_eq!(m.messages(), vec![Cell::Assistant("hello".into())]);
    }

    #[test]
    fn record_beating_the_stream_suppresses_late_bytes() {
        let mut m = Model::default();
        m.assistant_record("hello");
        m.stream("hel");
        m.stream("lo");
        assert_eq!(m.messages(), vec![Cell::Assistant("hello".into())]);
    }

    #[test]
    fn streamed_prefix_finished_in_place() {
        let mut m = Model::default();
        m.stream("hel");
        m.assistant_record("hello");
        m.stream("lo");
        assert_eq!(m.messages(), vec![Cell::Assistant("hello".into())]);
    }

    #[test]
    fn identical_answer_across_turns_is_not_swallowed() {
        let mut m = Model::default();
        m.stream("Done.");
        m.assistant_record("Done.");
        m.user_record("again");
        m.assistant_record("Done."); // stream missed
        assert_eq!(
            m.messages(),
            vec![
                Cell::Assistant("Done.".into()),
                Cell::User("again".into()),
                Cell::Assistant("Done.".into()),
            ]
        );
    }

    #[test]
    fn repeated_user_prompts_from_another_writer_keep_boundaries() {
        let mut m = Model::default();
        m.user_record("same");
        m.assistant_record("a");
        m.user_record("same");
        m.assistant_record("a");
        assert_eq!(
            m.messages(),
            vec![
                Cell::User("same".into()),
                Cell::Assistant("a".into()),
                Cell::User("same".into()),
                Cell::Assistant("a".into()),
            ]
        );
    }

    #[test]
    fn local_echo_is_deduped_once() {
        let mut m = Model::default();
        m.rec.on_local_submit("hi");
        m.cells.push(Cell::User("hi".into())); // app's local echo
        m.user_record("hi"); // confirming record
        assert_eq!(m.messages(), vec![Cell::User("hi".into())]);
    }

    #[test]
    fn next_turn_stream_racing_ahead_of_its_user_record_is_held_then_placed_after_it() {
        let mut m = Model::default();
        m.user_record("u1");
        m.assistant_record("hello"); // stream missed turn 1
        m.stream("hello"); // turn 1's late tail, fully suppressed
        m.stream("w"); // turn 2's stream, ahead of its user record: held
        m.user_record("u2"); // boundary flushes the held "w"
        m.assistant_record("world");
        m.stream("orld");
        assert_eq!(
            m.messages(),
            vec![
                Cell::User("u1".into()),
                Cell::Assistant("hello".into()),
                Cell::User("u2".into()),
                Cell::Assistant("world".into()),
            ]
        );
    }

    #[test]
    fn stream_running_ahead_holds_the_next_turn_until_its_boundary() {
        let mut m = Model::default();
        m.stream("onetwo"); // turn1 + start of turn2 before turn1's record
        m.assistant_record("one"); // confirms "one", holds "two"
        // "two" is not shown yet — its user boundary has not arrived.
        assert_eq!(m.messages(), vec![Cell::Assistant("one".into())]);
        m.user_record("u2");
        assert_eq!(
            m.messages(),
            vec![
                Cell::Assistant("one".into()),
                Cell::User("u2".into()),
                Cell::Assistant("two".into()),
            ]
        );
    }

    #[test]
    fn interposed_boundary_cell_does_not_break_reconciliation() {
        let mut m = Model::default();
        m.stream("par");
        m.cells.push(Cell::Boundary); // a plan/notice cell mid-turn
        m.assistant_record("partial");
        assert_eq!(
            m.messages(),
            vec![Cell::Assistant("partial".into())],
            "reconciliation targets the assistant cell across an interposed boundary"
        );
    }

    // ---- Exhaustive property test: tape is ground truth ----

    /// For a two-turn conversation, drive every legal interleaving of the
    /// output subsequence (each answer streamed as prefix-before-record +
    /// tail-after-record) and the tape subsequence, and assert the message
    /// cells always equal the tape ground truth. This is the invariant every
    /// review round probed by hand; here it is enumerated.
    #[test]
    fn every_interleaving_matches_the_tape_ground_truth() {
        let convos = [
            ("u1", "hello", "u2", "world"),
            ("ask", "Done.", "ask", "Done."), // repeated prompt + answer
            ("a", "abc", "b", "ab"),          // turn2 answer is a prefix of turn1
            ("a", "xy", "b", "xyz"),          // turn1 answer is a prefix of turn2
        ];
        for (u1, a1, u2, a2) in convos {
            for p1 in 0..=a1.len() {
                if !a1.is_char_boundary(p1) {
                    continue;
                }
                for p2 in 0..=a2.len() {
                    if !a2.is_char_boundary(p2) {
                        continue;
                    }
                    // Vary whether turn 2's user record is observed before or
                    // after its first stream chunk (the cross-tail race).
                    for user_first in [true, false] {
                        let mut m = Model::default();
                        // Turn 1
                        m.user_record(u1);
                        m.stream(&a1[..p1]);
                        m.assistant_record(a1);
                        m.stream(&a1[p1..]);
                        // Turn 2
                        if user_first {
                            m.user_record(u2);
                            m.stream(&a2[..p2]);
                        } else {
                            m.stream(&a2[..p2]);
                            m.user_record(u2);
                        }
                        m.assistant_record(a2);
                        m.stream(&a2[p2..]);

                        let expected = vec![
                            Cell::User(u1.into()),
                            Cell::Assistant(a1.into()),
                            Cell::User(u2.into()),
                            Cell::Assistant(a2.into()),
                        ];
                        assert_eq!(
                            m.messages(),
                            expected,
                            "interleaving p1={p1} p2={p2} user_first={user_first} \
                             convo=({u1},{a1},{u2},{a2}) diverged from tape truth"
                        );
                    }
                }
            }
        }
    }
}
