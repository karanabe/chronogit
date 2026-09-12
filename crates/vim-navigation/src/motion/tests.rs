use super::{CountSource, Cursor, Motion, MotionKind};
use super::{Viewport, apply};

fn motion(kind: MotionKind, count: usize) -> Motion {
    let source = if count == 1 {
        CountSource::Implicit
    } else {
        CountSource::Explicit
    };
    Motion::new(kind).counted(count, source)
}

#[test]
fn word_and_big_word_motions_follow_vim_boundaries() {
    let lines = ["one.two  three", "", "four-five"];
    let mut viewport = Viewport::new(0, 0, 10, 80, 0);
    let mut cursor = Cursor::new(0, 0);
    cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        motion(MotionKind::WordForward, 1),
    );
    assert_eq!(cursor, Cursor::new(0, 3));
    cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        motion(MotionKind::WordForward, 2),
    );
    assert_eq!(cursor, Cursor::new(0, 9));
    cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        motion(MotionKind::BigWordForward, 1),
    );
    assert_eq!(cursor, Cursor::new(1, 0));
    cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        motion(MotionKind::WordEndForward, 1),
    );
    assert_eq!(cursor, Cursor::new(2, 3));
    cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        motion(MotionKind::WordEndBackward, 1),
    );
    assert_eq!(cursor, Cursor::new(1, 0));
}

#[test]
fn sentence_motions_skip_all_closing_punctuation() {
    let lines = ["First.\")] Next.", "Another."];
    let mut viewport = Viewport::new(0, 0, 2, 80, 0);
    let cursor = apply(
        &lines,
        Cursor::new(0, 0),
        &mut viewport,
        Motion::new(MotionKind::SentenceForward),
    );
    assert_eq!(cursor, Cursor::new(0, 10));
    let cursor = apply(
        &lines,
        Cursor::new(1, 0),
        &mut viewport,
        Motion::new(MotionKind::SentenceBackward),
    );
    assert_eq!(cursor, Cursor::new(0, 10));
}

#[test]
fn oversized_counts_stop_at_text_boundaries() {
    let lines = ["one 界", "two"];
    for kind in [
        MotionKind::Left,
        MotionKind::Right,
        MotionKind::LeftWrap,
        MotionKind::RightWrap,
        MotionKind::WordForward,
        MotionKind::WordBackward,
        MotionKind::WordEndForward,
        MotionKind::WordEndBackward,
    ] {
        let mut viewport = Viewport::new(0, 0, 10, 80, 0);
        let cursor = apply(
            &lines,
            Cursor::new(0, 0),
            &mut viewport,
            motion(kind, usize::MAX),
        );
        let expected = match kind {
            MotionKind::Right => Cursor::new(0, 4),
            MotionKind::RightWrap | MotionKind::WordForward | MotionKind::WordEndForward => {
                Cursor::new(1, 2)
            }
            _ => Cursor::new(0, 0),
        };
        assert_eq!(cursor, expected, "{kind:?}");
    }
}

#[test]
fn long_and_replaced_snapshots_bound_work_by_content_not_count() {
    let owned = (0..20_000)
        .map(|line| format!("line {line} has words and 界"))
        .collect::<Vec<_>>();
    let lines = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let mut viewport = Viewport::new(0, 0, 30, 100, 0);
    let cursor = apply(
        &lines,
        Cursor::new(0, 0),
        &mut viewport,
        motion(MotionKind::WordForward, usize::MAX),
    );
    assert_eq!(cursor.line(), lines.len().saturating_sub(1));
    assert_eq!(
        cursor.byte_column(),
        lines[lines.len().saturating_sub(1)].len() - 3
    );

    let replacement = ["short", "界"];
    let cursor = apply(
        &replacement,
        cursor,
        &mut viewport,
        motion(MotionKind::Down, usize::MAX),
    );
    assert_eq!(cursor, Cursor::new(1, 0));
    assert_eq!(viewport.top(), 1);
}

#[test]
fn till_repeats_skip_only_an_adjacent_target_that_would_not_move() {
    let lines = ["axbx cxdx"];
    let mut viewport = Viewport::new(0, 0, 10, 80, 0);
    let mut cursor = Cursor::new(0, 0);
    let forward = Motion::new(MotionKind::TillForward).targeting('x');
    cursor = apply(&lines, cursor, &mut viewport, forward);
    assert_eq!(cursor.byte_column(), 0);
    cursor = apply(&lines, cursor, &mut viewport, forward.repeating());
    assert_eq!(cursor.byte_column(), 2);
    cursor = apply(&lines, cursor, &mut viewport, forward.repeating());
    assert_eq!(cursor.byte_column(), 5);
    let backward = Motion::new(MotionKind::TillBackward)
        .targeting('x')
        .repeating();
    cursor = apply(&lines, cursor, &mut viewport, backward);
    assert_eq!(cursor.byte_column(), 4);
    cursor = apply(
        &lines,
        Cursor::new(0, 0),
        &mut viewport,
        forward.repeating(),
    );
    assert_eq!(cursor.byte_column(), 2);
}

#[test]
fn character_search_and_matching_pairs_keep_utf8_boundaries() {
    let lines = ["a界(b(c)d)e"];
    let mut viewport = Viewport::new(0, 0, 4, 20, 0);
    let find = Motion::new(MotionKind::FindForward)
        .counted(2, CountSource::Explicit)
        .targeting('c');
    let cursor = apply(&lines, Cursor::new(0, 0), &mut viewport, find);
    assert_eq!(cursor, Cursor::new(0, 0));
    let cursor = apply(
        &lines,
        Cursor::new(0, "a界".len()),
        &mut viewport,
        motion(MotionKind::MatchingPair, 1),
    );
    assert_eq!(cursor, Cursor::new(0, "a界(b(c)d".len()));

    let cursor = apply(
        &lines,
        Cursor::new(0, "a界(b(c)".len()),
        &mut viewport,
        motion(MotionKind::MatchingPairBackward, 1),
    );
    assert_eq!(cursor, Cursor::new(0, "a界(b".len()));
}

#[test]
fn space_and_backspace_wrap_lines_without_changing_h_and_l() {
    let lines = ["ab", "", "界x"];
    let mut viewport = Viewport::new(0, 0, 4, 20, 0);
    let cursor = apply(
        &lines,
        Cursor::new(0, 1),
        &mut viewport,
        motion(MotionKind::RightWrap, 2),
    );
    assert_eq!(cursor, Cursor::new(2, 0));
    let cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        motion(MotionKind::LeftWrap, 2),
    );
    assert_eq!(cursor, Cursor::new(0, 1));

    assert_eq!(
        apply(
            &lines,
            Cursor::new(0, 1),
            &mut viewport,
            motion(MotionKind::Right, 1),
        ),
        Cursor::new(0, 1)
    );
    assert_eq!(
        apply(
            &lines,
            Cursor::new(2, 0),
            &mut viewport,
            motion(MotionKind::Left, 1),
        ),
        Cursor::new(2, 0)
    );
}

#[test]
fn percent_matches_comments_and_nested_preprocessor_conditionals() {
    let comments = ["a /* one", "two */ b"];
    let mut viewport = Viewport::new(0, 0, 8, 40, 0);
    let cursor = apply(
        &comments,
        Cursor::new(0, 2),
        &mut viewport,
        motion(MotionKind::MatchingPair, 1),
    );
    assert_eq!(cursor, Cursor::new(1, 5));
    let cursor = apply(
        &comments,
        Cursor::new(1, 4),
        &mut viewport,
        motion(MotionKind::MatchingPair, 1),
    );
    assert_eq!(cursor, Cursor::new(0, 2));

    let directives = ["#if A", "#if B", "#else", "#endif", "#else", "#endif"];
    let cursor = apply(
        &directives,
        Cursor::new(0, 0),
        &mut viewport,
        motion(MotionKind::MatchingPair, 1),
    );
    assert_eq!(cursor, Cursor::new(4, 0));
    let cursor = apply(
        &directives,
        Cursor::new(5, 0),
        &mut viewport,
        motion(MotionKind::MatchingPair, 1),
    );
    assert_eq!(cursor, Cursor::new(0, 0));

    let cursor = apply(
        &directives,
        Cursor::new(3, 0),
        &mut viewport,
        motion(MotionKind::PreprocessorBackward, 1),
    );
    assert_eq!(cursor, Cursor::new(2, 0));
    let cursor = apply(
        &directives,
        Cursor::new(1, 0),
        &mut viewport,
        motion(MotionKind::PreprocessorForward, 1),
    );
    assert_eq!(cursor, Cursor::new(2, 0));
}

#[test]
fn an_explicit_half_page_count_is_a_line_count() {
    let owned = (0..30).map(|line| line.to_string()).collect::<Vec<_>>();
    let lines = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let mut viewport = Viewport::new(0, 0, 10, 80, 0);
    let cursor = apply(
        &lines,
        Cursor::new(0, 0),
        &mut viewport,
        Motion::new(MotionKind::HalfPageDown).counted(2, CountSource::Explicit),
    );
    assert_eq!(cursor, Cursor::new(2, 0));

    let cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        Motion::new(MotionKind::HalfPageDown),
    );
    assert_eq!(cursor, Cursor::new(7, 0));
}

#[test]
fn paragraph_motions_cross_runs_of_empty_lines_like_vim() {
    let lines = ["one", "two", "", "", "three", "four", "", "five"];
    let mut viewport = Viewport::new(0, 0, 8, 40, 0);
    let forward = |line, viewport: &mut Viewport| {
        apply(
            &lines,
            Cursor::new(line, 0),
            viewport,
            motion(MotionKind::ParagraphForward, 1),
        )
    };
    assert_eq!(forward(0, &mut viewport), Cursor::new(2, 0));
    assert_eq!(forward(2, &mut viewport), Cursor::new(6, 0));
    let backward = |line, viewport: &mut Viewport| {
        apply(
            &lines,
            Cursor::new(line, 0),
            viewport,
            motion(MotionKind::ParagraphBackward, 1),
        )
    };
    assert_eq!(backward(4, &mut viewport), Cursor::new(3, 0));
    assert_eq!(backward(3, &mut viewport), Cursor::new(0, 0));

    let no_trailing_blank = ["one", "two"];
    let cursor = apply(
        &no_trailing_blank,
        Cursor::new(0, 0),
        &mut viewport,
        motion(MotionKind::ParagraphForward, 1),
    );
    assert_eq!(cursor, Cursor::new(1, 2));
}

#[test]
fn page_and_window_motions_use_the_actual_viewport() {
    let owned = (0..40)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>();
    let lines = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let mut viewport = Viewport::new(10, 0, 8, 40, 0);
    let cursor = apply(
        &lines,
        Cursor::new(12, 0),
        &mut viewport,
        motion(MotionKind::WindowBottom, 1),
    );
    assert_eq!(cursor.line(), 17);
    let cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        motion(MotionKind::PageDown, 1),
    );
    assert_eq!(cursor.line(), 16);
    assert_eq!(viewport.top, 16);
}

#[test]
fn z_commands_preserve_or_reset_columns_and_horizontal_scroll_follows_cursor() {
    let lines = ["  zero", "    one", "  two", "three", "four", "five"];
    let mut viewport = Viewport::new(0, 0, 5, 4, 0);
    let cursor = apply(
        &lines,
        Cursor::new(0, 4),
        &mut viewport,
        Motion::new(MotionKind::CursorToWindowTop).counted(2, CountSource::Explicit),
    );
    assert_eq!(cursor, Cursor::new(1, 4));
    assert_eq!(viewport.top, 1);

    let cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        Motion::new(MotionKind::CursorToWindowMiddleFirstNonBlank)
            .counted(3, CountSource::Explicit),
    );
    assert_eq!(cursor, Cursor::new(2, 2));
    assert_eq!(viewport.top, 0);

    let cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        Motion::new(MotionKind::PreviousWindowBottom).counted(6, CountSource::Explicit),
    );
    assert_eq!(cursor, Cursor::new(1, 4));
    assert_eq!(viewport.top, 0);

    let horizontal = ["abcdefghij"];
    let mut viewport = Viewport::new(0, 0, 1, 4, 0);
    let cursor = apply(
        &horizontal,
        Cursor::new(0, 0),
        &mut viewport,
        Motion::new(MotionKind::ScrollColumnRight).counted(3, CountSource::Explicit),
    );
    assert_eq!(viewport.left, 3);
    assert_eq!(cursor, Cursor::new(0, 3));
}

#[test]
fn vertical_motions_preserve_the_wanted_display_column_across_short_lines() {
    let lines = ["abcdef", "x", "abcdef"];
    let mut viewport = Viewport::new(0, 0, 3, 20, 0);
    let cursor = apply(
        &lines,
        Cursor::new(0, 4),
        &mut viewport,
        motion(MotionKind::Down, 1),
    );
    assert_eq!(cursor, Cursor::new(1, 0));
    let cursor = apply(&lines, cursor, &mut viewport, motion(MotionKind::Down, 1));
    assert_eq!(cursor, Cursor::new(2, 4));

    let cursor = apply(
        &lines,
        cursor,
        &mut viewport,
        motion(MotionKind::LineEnd, 1),
    );
    let cursor = apply(&lines, cursor, &mut viewport, motion(MotionKind::Up, 1));
    assert_eq!(cursor, Cursor::new(1, 0));
    let cursor = apply(&lines, cursor, &mut viewport, motion(MotionKind::Up, 1));
    assert_eq!(cursor, Cursor::new(0, 5));
}

#[test]
fn counted_screen_end_motions_move_down_before_selecting_the_column() {
    let lines = ["first", "second   ", "third"];
    let mut viewport = Viewport::new(0, 0, 3, 20, 0);
    let cursor = apply(
        &lines,
        Cursor::new(0, 0),
        &mut viewport,
        Motion::new(MotionKind::ScreenLineEnd).counted(2, CountSource::Explicit),
    );
    assert_eq!(cursor, Cursor::new(1, 8));

    let cursor = apply(
        &lines,
        Cursor::new(0, 0),
        &mut viewport,
        Motion::new(MotionKind::ScreenLastNonBlank).counted(2, CountSource::Explicit),
    );
    assert_eq!(cursor, Cursor::new(1, 5));
}

#[test]
fn screen_column_motions_account_for_a_partly_scrolled_gutter() {
    let lines = ["  abcdefghijklmnop  "];
    let mut viewport = Viewport::new(0, 10, 1, 10, 8);
    let start = apply(
        &lines,
        Cursor::new(0, 8),
        &mut viewport,
        motion(MotionKind::ScreenLineStart, 1),
    );
    assert_eq!(start, Cursor::new(0, 2));
    let middle = apply(
        &lines,
        start,
        &mut viewport,
        motion(MotionKind::ScreenMiddle, 1),
    );
    assert_eq!(middle, Cursor::new(0, 7));
    let end = apply(
        &lines,
        middle,
        &mut viewport,
        motion(MotionKind::ScreenLineEnd, 1),
    );
    assert_eq!(end, Cursor::new(0, 11));
    let non_blank = apply(
        &lines,
        middle,
        &mut viewport,
        motion(MotionKind::ScreenLastNonBlank, 1),
    );
    assert_eq!(non_blank, Cursor::new(0, 11));
}

#[test]
fn byte_offsets_and_diff_change_motions_are_count_aware() {
    let lines = ["abc", "+first", "+more", " context", "-second"];
    let mut viewport = Viewport::new(0, 0, 4, 40, 0);
    let cursor = apply(
        &lines,
        Cursor::new(0, 0),
        &mut viewport,
        motion(MotionKind::ByteOffset, 6),
    );
    assert_eq!(cursor, Cursor::new(1, 1));
    let cursor = apply(
        &lines,
        Cursor::new(0, 0),
        &mut viewport,
        motion(MotionKind::DiffChangeForward, 2),
    );
    assert_eq!(cursor, Cursor::new(4, 0));

    let cursor = apply(
        &lines,
        Cursor::new(2, 2),
        &mut viewport,
        motion(MotionKind::DiffChangeBackward, 1),
    );
    assert_eq!(cursor, Cursor::new(1, 0));
}
