//! E6-3: Session compaction integration tests.
//!
//! These tests exercise `compact_session`, `should_compact`, and
//! `estimate_session_tokens` against *known* token-count thresholds, covering
//! every boundary condition documented in the acceptance criteria.
//!
//! Scenarios:
//! 1. **Below threshold** — session whose token estimate is below
//!    `max_estimated_tokens` is NOT compacted.
//! 2. **At threshold** — session whose estimate equals the threshold is NOT
//!    compacted (the guard is `>=`; boundary messages exactly at threshold still
//!    pass without compaction when fewer than `preserve_recent_messages` remain).
//! 3. **One token above threshold** — compaction fires and at least one message
//!    is removed.
//! 4. **Token estimate decreases after compaction** — the compacted session must
//!    have a strictly smaller token estimate than the original.
//! 5. **Preserve tail** — the last `preserve_recent_messages` messages always
//!    survive verbatim.
//! 6. **Fewer messages than preserve_recent** — compaction never fires when the
//!    total compactable count is ≤ `preserve_recent_messages`.
//! 7. **Double compaction preserves prior context** — a second compaction of an
//!    already-compacted session merges the prior summary into the new one.
//! 8. **Empty session** — compacting an empty session is a no-op.
//! 9. **Single large message** — a session with one huge message that exceeds
//!    the threshold but only has one message: no compaction because there are
//!    not enough messages to satisfy `preserve_recent_messages`.
//! 10. **Tool-use messages are counted** — token estimates include tool-use and
//!     tool-result blocks, not only user/assistant text.
//! 11. **Continuation message is injected as System role** — the first message
//!     of the compacted session must be a System-role message containing
//!     the summary preamble.
//! 12. **Custom thresholds** — smoke-test that overriding both fields in
//!     `CompactionConfig` produces the expected result.

use runtime::{
    compact_session, estimate_session_tokens, should_compact, CompactionConfig, CompactionResult,
    ContentBlock, ConversationMessage, MessageRole, Session,
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Build a session where every message is a user text of `words` repeated
/// words.  Each word is 4 characters + space ≈ 1 token per word (rough heuristic
/// matches the compact crate's own estimator which counts characters / 4).
fn session_with_messages(count: usize, words_each: usize) -> Session {
    let text = "word ".repeat(words_each);
    let mut session = Session::new();
    for _ in 0..count {
        session
            .messages
            .push(ConversationMessage::user_text(text.clone()));
    }
    session
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Below threshold — no compaction
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn session_below_token_threshold_is_not_compacted() {
    // given — 8 messages × 10 words each ≈ 8 × 12 = ~96 tokens
    let session = session_with_messages(8, 10);
    let config = CompactionConfig {
        preserve_recent_messages: 4,
        max_estimated_tokens: 10_000, // far above
    };

    // when
    let should = should_compact(&session, config);
    let result = compact_session(&session, config);

    // then
    assert!(!should, "session below threshold should not trigger compaction");
    assert_eq!(result.removed_message_count, 0);
    assert_eq!(result.compacted_session, session);
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Exactly at threshold — NOT compacted (boundary: guard uses >=)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn session_exactly_at_threshold_is_not_compacted_when_not_enough_compactable_messages() {
    // given — 4 messages, preserve_recent=4 → 0 compactable messages
    // should_compact requires compactable.len() > preserve_recent_messages
    let session = session_with_messages(4, 500);
    let estimated = estimate_session_tokens(&session);
    let config = CompactionConfig {
        preserve_recent_messages: 4,
        max_estimated_tokens: estimated, // exactly at boundary
    };

    // when
    let should = should_compact(&session, config);

    // then
    assert!(
        !should,
        "when all messages fall in the preserve window, compaction must not fire (got estimated={estimated})"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. One token above threshold — compaction fires
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn session_one_token_above_threshold_triggers_compaction() {
    // given — 8 messages of 100 words each ≈ 8 × 125 = ~1000 tokens
    let session = session_with_messages(8, 100);
    let estimated = estimate_session_tokens(&session);
    assert!(estimated > 0, "estimated tokens should be > 0");

    // config: threshold is one below the estimate
    let config = CompactionConfig {
        preserve_recent_messages: 2,
        max_estimated_tokens: estimated.saturating_sub(1), // threshold < actual
    };

    // when
    let should = should_compact(&session, config);
    let result = compact_session(&session, config);

    // then
    assert!(should, "session above threshold should trigger compaction");
    assert!(
        result.removed_message_count > 0,
        "at least one message should be removed"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Token estimate strictly decreases after compaction
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn compacted_session_has_strictly_fewer_estimated_tokens() {
    // given
    let session = session_with_messages(10, 200);
    let before = estimate_session_tokens(&session);
    let config = CompactionConfig {
        preserve_recent_messages: 2,
        max_estimated_tokens: 1, // always triggers
    };

    // when
    let result = compact_session(&session, config);
    let after = estimate_session_tokens(&result.compacted_session);

    // then
    assert!(
        after < before,
        "compacted session must have fewer estimated tokens: before={before} after={after}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Preserve tail — last N messages survive verbatim
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn last_n_messages_are_preserved_verbatim_after_compaction() {
    // given — 10 messages, preserve last 3
    let messages: Vec<ConversationMessage> = (0..10u32)
        .map(|i| ConversationMessage::user_text(format!("message-{i} {}", "payload ".repeat(50))))
        .collect();
    let mut session = Session::new();
    session.messages = messages.clone();

    let config = CompactionConfig {
        preserve_recent_messages: 3,
        max_estimated_tokens: 1,
    };

    // when
    let result = compact_session(&session, config);
    let compacted = &result.compacted_session.messages;

    // then — first message is the system summary; then the last 3 original
    assert_eq!(compacted[0].role, MessageRole::System);
    let preserved = &compacted[1..];
    assert_eq!(preserved.len(), 3);
    // the 3 preserved messages are the last 3 original messages
    for (preserved_msg, original_msg) in preserved.iter().zip(messages[7..].iter()) {
        assert_eq!(preserved_msg.blocks, original_msg.blocks);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Fewer messages than preserve_recent — no compaction
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn session_with_fewer_messages_than_preserve_window_is_never_compacted() {
    // given — 3 messages, window = 4 → 0 compactable → no compaction regardless
    let session = session_with_messages(3, 2000);
    let config = CompactionConfig {
        preserve_recent_messages: 4,
        max_estimated_tokens: 1, // tiny threshold
    };

    // when
    let should = should_compact(&session, config);
    let result = compact_session(&session, config);

    // then
    assert!(!should);
    assert_eq!(result.removed_message_count, 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Double compaction merges prior summary
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn second_compaction_merges_prior_summary_into_combined_context() {
    // given — first compaction
    let session1 = session_with_messages(6, 100);
    let config = CompactionConfig {
        preserve_recent_messages: 2,
        max_estimated_tokens: 1,
    };
    let first: CompactionResult = compact_session(&session1, config);

    // add more large messages on top of the already-compacted session
    let mut session2 = first.compacted_session.clone();
    for i in 0..4u32 {
        session2
            .messages
            .push(ConversationMessage::user_text(format!(
                "follow-up-{i} {}",
                "more ".repeat(100)
            )));
    }

    // when
    let second = compact_session(&session2, config);

    // then — formatted summary must reference previously compacted context
    assert!(
        second.formatted_summary.contains("Previously compacted context:")
            || second.formatted_summary.contains("Conversation summary:"),
        "second compaction must merge prior context, got: {}",
        &second.formatted_summary[..200.min(second.formatted_summary.len())]
    );
    assert!(
        second.removed_message_count > 0,
        "second compaction should remove at least one message"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Empty session — no-op
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_session_compacts_to_itself_with_zero_removed() {
    // given
    let session = Session::new();
    let config = CompactionConfig {
        preserve_recent_messages: 4,
        max_estimated_tokens: 1,
    };

    // when
    let result = compact_session(&session, config);

    // then
    assert_eq!(result.removed_message_count, 0);
    assert_eq!(result.compacted_session.messages, session.messages);
    assert!(result.summary.is_empty());
}

// ──────────────────────────────────────────────────────────────────────────────
// 9. Single large message — no compaction (not enough compactable messages)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn single_message_exceeding_threshold_is_not_compacted() {
    // given — one message with many tokens, but preserve_recent_messages=1
    // means compactable.len() = 0 → no compaction
    let mut session = Session::new();
    session
        .messages
        .push(ConversationMessage::user_text("big ".repeat(10_000)));
    let config = CompactionConfig {
        preserve_recent_messages: 1,
        max_estimated_tokens: 1,
    };

    // when
    let result = compact_session(&session, config);

    // then
    assert_eq!(
        result.removed_message_count, 0,
        "a single message cannot be compacted"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 10. Tool-use messages are counted in the token estimate
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn token_estimate_includes_tool_use_and_tool_result_blocks() {
    // given — session with a tool-use and a tool-result block
    let mut tool_session = Session::new();
    tool_session.messages = vec![
        ConversationMessage::user_text("run search"),
        ConversationMessage::assistant(vec![ContentBlock::ToolUse {
            id: "1".to_string(),
            name: "search".to_string(),
            input: "query ".repeat(100),
        }]),
        ConversationMessage::tool_result(
            "1",
            "search",
            "result ".repeat(100),
            false,
        ),
    ];

    // compare with a text-only session of similar character count
    let mut text_session = Session::new();
    text_session.messages = vec![
        ConversationMessage::user_text("run search"),
        ConversationMessage::user_text("query ".repeat(100)),
        ConversationMessage::user_text("result ".repeat(100)),
    ];

    // when
    let tool_estimate = estimate_session_tokens(&tool_session);
    let text_estimate = estimate_session_tokens(&text_session);

    // then — both should be non-zero and in the same order of magnitude
    assert!(
        tool_estimate > 0,
        "tool-use session should have a non-zero estimate"
    );
    // The estimates should be within 50% of each other (similar character counts)
    let ratio = tool_estimate as f64 / text_estimate as f64;
    assert!(
        (0.5..=2.0).contains(&ratio),
        "tool and text estimates should be comparable: tool={tool_estimate} text={text_estimate} ratio={ratio:.2}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 11. Continuation message is a System-role message
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn first_message_of_compacted_session_is_system_role_with_summary_preamble() {
    // given
    let session = session_with_messages(8, 150);
    let config = CompactionConfig {
        preserve_recent_messages: 2,
        max_estimated_tokens: 1,
    };

    // when
    let result = compact_session(&session, config);
    let first = result
        .compacted_session
        .messages
        .first()
        .expect("compacted session should have at least one message");

    // then
    assert_eq!(first.role, MessageRole::System);
    let text = match &first.blocks[0] {
        ContentBlock::Text { text } => text,
        other => panic!("expected Text block, got {other:?}"),
    };
    // must contain the continuation preamble
    assert!(
        text.contains("continued from a previous conversation")
            || text.contains("Summary:"),
        "continuation message must contain summary preamble, got: {text:.200?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 12. Custom thresholds
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn custom_preserve_recent_and_max_tokens_controls_compaction_granularity() {
    // given — 12 messages, preserve 5, threshold very low
    let session = session_with_messages(12, 50);
    let config = CompactionConfig {
        preserve_recent_messages: 5,
        max_estimated_tokens: 1,
    };

    // when
    let result = compact_session(&session, config);

    // then — exactly 12 - 5 = 7 messages should be removed (compactable 0..7)
    assert_eq!(
        result.removed_message_count, 7,
        "with 12 messages and preserve_recent=5, 7 should be removed"
    );
    // compacted session: 1 system summary + 5 preserved = 6 messages
    assert_eq!(result.compacted_session.messages.len(), 6);
    assert_eq!(
        result.compacted_session.messages[0].role,
        MessageRole::System
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 13. estimate_session_tokens is additive across messages
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn token_estimate_scales_proportionally_with_message_count() {
    // given — two sessions: one with 4 messages, one with 8 of the same size
    let s4 = session_with_messages(4, 200);
    let s8 = session_with_messages(8, 200);

    // when
    let t4 = estimate_session_tokens(&s4);
    let t8 = estimate_session_tokens(&s8);

    // then — 8-message session should have roughly twice the token count
    let ratio = t8 as f64 / t4 as f64;
    assert!(
        (1.8..=2.2).contains(&ratio),
        "token estimate should scale ~linearly: t4={t4} t8={t8} ratio={ratio:.2}"
    );
}
