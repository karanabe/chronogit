//! `ChronoGit` coordinate adapter for the reusable `vim-navigation` crate.

use crate::app::VimMotion;
use crate::domain::SourcePosition;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Viewport {
    pub(crate) top: usize,
    pub(crate) left: usize,
    pub(crate) height: usize,
    pub(crate) width: usize,
    pub(crate) gutter: usize,
    pub(crate) desired_column: Option<usize>,
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
        }
    }

    pub(crate) fn with_desired_column(mut self, desired_column: Option<usize>) -> Self {
        self.desired_column = desired_column;
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
    let cursor = vim_navigation::apply(
        lines,
        vim_navigation::Cursor::new(
            usize::try_from(position.line()).unwrap_or(usize::MAX),
            position.byte_column(),
        ),
        &mut navigation_viewport,
        motion,
    );
    viewport.update_from(&navigation_viewport);
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
}

#[cfg(test)]
mod tests {
    use super::{Viewport, apply};
    use crate::app::{VimCountSource, VimMotion, VimMotionKind};
    use crate::domain::SourcePosition;

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
