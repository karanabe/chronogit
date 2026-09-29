//! `ChronoGit` coordinate adapter for the reusable `vim-navigation` crate.

use crate::app::{VimCountSource, VimMotion, VimMotionKind};
use crate::domain::SourcePosition;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Viewport {
    pub(crate) top: usize,
    pub(crate) left: usize,
    pub(crate) height: usize,
    pub(crate) width: usize,
    pub(crate) gutter: usize,
    pub(crate) desired_column: Option<usize>,
    scrolloff: usize,
}

impl Viewport {
    pub(crate) fn new(top: usize, left: usize, height: usize, width: usize, gutter: usize) -> Self {
        Self {
            top,
            left,
            height: height.max(1),
            width: width.max(1),
            gutter,
            desired_column: None,
            scrolloff: 0,
        }
    }

    pub(crate) fn with_desired_column(mut self, desired_column: Option<usize>) -> Self {
        self.desired_column = desired_column;
        self
    }

    pub(crate) fn with_scrolloff(mut self, scrolloff: usize) -> Self {
        self.scrolloff = scrolloff;
        self
    }

    fn navigation_viewport(self) -> vim_navigation::Viewport {
        vim_navigation::Viewport::new(self.top, self.left, self.height, self.width, self.gutter)
            .with_desired_column(self.desired_column)
    }

    fn update_from(&mut self, viewport: &vim_navigation::Viewport) {
        self.top = viewport.top();
        self.left = viewport.left();
        self.desired_column = viewport.desired_column();
    }
}

pub(crate) fn apply(
    lines: &[&str],
    position: SourcePosition,
    viewport: &mut Viewport,
    motion: VimMotion,
) -> SourcePosition {
    let mut navigation_viewport = viewport.navigation_viewport();
    let mut cursor = vim_navigation::apply(
        lines,
        vim_navigation::Cursor::new(
            usize::try_from(position.line()).unwrap_or(usize::MAX),
            position.byte_column(),
        ),
        &mut navigation_viewport,
        motion,
    );
    viewport.update_from(&navigation_viewport);
    // Explicit screen scrolling keeps the requested window and brings the
    // cursor into its context band, as Vim does with a nonzero scrolloff.
    if matches!(
        motion.kind(),
        VimMotionKind::ScrollLineUp
            | VimMotionKind::ScrollLineDown
            | VimMotionKind::PageUp
            | VimMotionKind::PageDown
            | VimMotionKind::WindowTop
            | VimMotionKind::WindowBottom
    ) {
        let margin = scroll_margin(viewport.height, viewport.scrolloff);
        let last = lines.len().saturating_sub(1);
        let first = if viewport.top == 0 {
            0
        } else {
            viewport.top.saturating_add(margin).min(last)
        };
        let bottom = if viewport.top.saturating_add(viewport.height) > last {
            last
        } else {
            viewport
                .top
                .saturating_add(viewport.height.saturating_sub(1 + margin))
                .min(last)
        };
        let line = cursor.line().clamp(first.min(bottom), bottom);
        if line != cursor.line() {
            // Reuse the motion engine to preserve display columns and UTF-8 boundaries.
            let kind = if line < cursor.line() {
                VimMotionKind::Up
            } else {
                VimMotionKind::Down
            };
            cursor = vim_navigation::apply(
                lines,
                cursor,
                &mut navigation_viewport,
                VimMotion::new(kind)
                    .counted(line.abs_diff(cursor.line()), VimCountSource::Explicit),
            );
            if matches!(
                motion.kind(),
                VimMotionKind::PageUp
                    | VimMotionKind::PageDown
                    | VimMotionKind::WindowTop
                    | VimMotionKind::WindowBottom
            ) {
                cursor = vim_navigation::apply(
                    lines,
                    cursor,
                    &mut navigation_viewport,
                    VimMotion::new(VimMotionKind::FirstNonBlank),
                );
                viewport.desired_column = navigation_viewport.desired_column();
            }
            viewport.left = navigation_viewport.left();
        }
    }
    viewport.top = scroll_top(
        cursor.line(),
        viewport.top,
        viewport.height,
        lines.len(),
        viewport.scrolloff,
    );
    SourcePosition::new(
        u32::try_from(cursor.line()).unwrap_or(u32::MAX),
        cursor.byte_column(),
    )
}

pub(crate) fn reveal(lines: &[&str], position: SourcePosition, viewport: &mut Viewport) {
    let mut navigation_viewport = viewport.navigation_viewport();
    vim_navigation::reveal(
        lines,
        vim_navigation::Cursor::new(
            usize::try_from(position.line()).unwrap_or(usize::MAX),
            position.byte_column(),
        ),
        &mut navigation_viewport,
    );
    viewport.update_from(&navigation_viewport);
    viewport.top = scroll_top(
        usize::try_from(position.line()).unwrap_or(usize::MAX),
        viewport.top,
        viewport.height,
        lines.len(),
        viewport.scrolloff,
    );
}

pub(crate) fn scroll_margin(height: usize, scrolloff: usize) -> usize {
    scrolloff.min(height.saturating_sub(1) / 2)
}

/// Follows a cursor only when it crosses the context band. Existing deliberate
/// placement (including blank rows after `zt`/`zz` near EOF) is retained.
pub(crate) fn scroll_top(
    cursor: usize,
    top: usize,
    height: usize,
    len: usize,
    scrolloff: usize,
) -> usize {
    let height = height.max(1);
    let last = len.saturating_sub(1);
    let cursor = cursor.min(last);
    let top = top.min(last);
    let margin = scroll_margin(height, scrolloff);
    let after = margin.min(last.saturating_sub(cursor));
    if cursor < top.saturating_add(margin) {
        cursor.saturating_sub(margin)
    } else if cursor.saturating_add(after) >= top.saturating_add(height) {
        cursor.saturating_add(after).saturating_sub(height - 1)
    } else {
        top
    }
}

#[cfg(test)]
mod tests {
    use super::{Viewport, apply, scroll_top};
    use crate::app::{VimCountSource, VimMotion, VimMotionKind};
    use crate::domain::SourcePosition;

    #[test]
    fn context_following_handles_empty_short_and_extreme_dimensions() {
        assert_eq!(scroll_top(usize::MAX, usize::MAX, 0, 0, usize::MAX), 0);
        assert_eq!(scroll_top(30, 20, 10, 100, 2), 23);
        assert_eq!(scroll_top(29, 23, 10, 100, 2), 23);
        assert_eq!(scroll_top(24, 23, 10, 100, 2), 22);
        assert_eq!(scroll_top(99, 0, 10, 100, 2), 90);
        assert_eq!(scroll_top(1, 50, 10, 2, 3), 0);
        assert_eq!(scroll_top(40, 0, 1, 100, 3), 40);
    }

    #[test]
    fn explicit_scroll_moves_the_window_and_keeps_the_cursor_in_the_context_band() {
        let lines = vec!["  日本"; 100];
        let mut viewport = Viewport::new(20, 0, 10, 80, 0).with_scrolloff(2);
        let cursor = apply(
            &lines,
            SourcePosition::new(22, 2),
            &mut viewport,
            VimMotion::new(VimMotionKind::ScrollLineDown),
        );
        assert_eq!(viewport.top, 21);
        assert_eq!(cursor, SourcePosition::new(23, 2));
        let cursor = apply(
            &lines,
            cursor,
            &mut viewport,
            VimMotion::new(VimMotionKind::WindowBottom),
        );
        assert_eq!(cursor, SourcePosition::new(28, 2));
        assert_eq!(viewport.top, 21);
        viewport.top = 0;
        let cursor = apply(
            &lines,
            SourcePosition::new(3, 2),
            &mut viewport,
            VimMotion::new(VimMotionKind::WindowTop),
        );
        assert_eq!(cursor.line(), 0);
        assert_eq!(viewport.top, 0);
    }

    #[test]
    fn adapts_chronogit_positions_without_changing_public_coordinates() {
        let lines = ["zero", "one two"];
        let mut viewport = Viewport::new(0, 0, 2, 20, 0);
        let cursor = apply(
            &lines,
            SourcePosition::new(0, 0),
            &mut viewport,
            VimMotion::new(VimMotionKind::WordForward).counted(2, VimCountSource::Explicit),
        );
        assert_eq!(cursor, SourcePosition::new(1, 4));
    }
}
