//! Normal-mode command resolution independent of key bindings.
//!
//! [`MotionState`] owns only state that is required to turn a motion template
//! into a completed [`Motion`]: a saturating decimal count, a pending `f`/`F`/
//! `t`/`T` character argument, and the last completed character search used by
//! `;` and `,`. It does not own terminal events, multi-key mapping timeouts,
//! search queries, marks, jump lists, buffers, cursors, or viewports.
//!
//! A caller normally feeds count digits to [`MotionState::push_count_digit`],
//! passes a bound motion to [`MotionState::finish`], and, after
//! [`MotionResolution::AwaitingTarget`], routes the next eligible character to
//! [`MotionState::accept_target`]. Mode/focus changes should call
//! [`MotionState::reset_pending`] so an incomplete command cannot leak into a
//! different input context. The last completed character search intentionally
//! survives that reset.
//!
//! # Example
//!
//! ```
//! use vim_navigation::command::{MotionResolution, MotionState};
//! use vim_navigation::motion::{Motion, MotionKind};
//!
//! let mut state = MotionState::new();
//! assert!(state.push_count_digit('3'));
//! let MotionResolution::Ready(motion) =
//!     state.finish(Motion::new(MotionKind::Right))
//! else {
//!     panic!("a non-targeted motion is immediately ready");
//! };
//! assert_eq!(motion.count(), 3);
//! ```

use crate::motion::FULL_PERCENT;
use crate::{CountSource, Motion, MotionKind};

/// The result of submitting a motion template to [`MotionState`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionResolution {
    /// A complete motion is ready for [`crate::motion::apply`] or a caller adapter.
    Ready(Motion),
    /// A find or till motion is stored internally and awaits one character.
    AwaitingTarget,
    /// `;` or `,` was requested before any character search was completed.
    ///
    /// The pending decimal count, if any, has already been consumed.
    Unavailable,
}

/// Normal-mode state shared across configurable key adapters.
///
/// This type owns the Vim semantics of decimal counts, `%` and `G` count
/// reinterpretation, find/till character arguments, and `;`/`,` repetition.
/// Terminal key events and application-specific bindings remain adapter
/// concerns. The state has no I/O and no caller-controlled panic path.
#[derive(Debug, Default)]
pub struct MotionState {
    count: Option<usize>,
    awaiting_target: Option<Motion>,
    last_character_search: Option<Motion>,
}

impl MotionState {
    /// Creates empty Normal-mode command state with no repeatable search.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: None,
            awaiting_target: None,
            last_character_search: None,
        }
    }

    /// Reports whether a non-zero decimal count has started.
    ///
    /// A leading `0` is deliberately not a count; callers can bind it to
    /// [`MotionKind::LineStart`].
    #[must_use]
    pub const fn has_count(&self) -> bool {
        self.count.is_some()
    }

    /// Adds an ASCII digit to the pending count with saturating arithmetic.
    ///
    /// Returns `false` for non-digits and for a leading zero, which lets an
    /// adapter interpret that key as Vim's `0` motion instead. Once the parsed
    /// value reaches [`usize::MAX`], further digits keep it at that value.
    pub fn push_count_digit(&mut self, digit: char) -> bool {
        let Some(value) = digit.to_digit(10) else {
            return false;
        };
        if value == 0 && self.count.is_none() {
            return false;
        }
        // Vim counts are conceptually unbounded decimal input. Saturation keeps
        // pathological input deterministic without overflow or a parse error.
        self.count = Some(
            self.count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(value as usize),
        );
        true
    }

    /// Takes a pending count for a non-motion command such as jump history.
    ///
    /// This does not cancel a pending character-argument motion or forget the
    /// last completed character search.
    pub fn take_count(&mut self) -> Option<usize> {
        self.count.take()
    }

    /// Reports whether a find or till target character is pending.
    #[must_use]
    pub const fn is_awaiting_target(&self) -> bool {
        self.awaiting_target.is_some()
    }

    /// Completes a motion template using the current count and repeat state.
    ///
    /// The pending count is always consumed. Vim-specific reinterpretations
    /// happen here: counted `%` becomes a clamped buffer percentage, and
    /// counted `G`/`gg` selects the supplied one-based line. Find/till motions
    /// without a target return [`MotionResolution::AwaitingTarget`]; those with
    /// a target immediately become the source for `;` and `,` repetition.
    ///
    /// While a target is pending, the caller must use [`Self::accept_target`]
    /// or [`Self::reset_pending`] before submitting an unrelated command.
    pub fn finish(&mut self, motion: Motion) -> MotionResolution {
        let count = self.count.take();
        let source = if count.is_some() {
            CountSource::Explicit
        } else {
            CountSource::Implicit
        };
        let mut motion = motion.counted(count.unwrap_or(1), source);
        // `%` changes meaning in Vim when prefixed by a count: it becomes an
        // absolute percentage rather than delimiter matching.
        if motion.kind() == MotionKind::MatchingPair && motion.has_explicit_count() {
            motion = Motion::new(MotionKind::BufferPercentage)
                .counted(motion.count().min(FULL_PERCENT), CountSource::Explicit);
        }
        // A count before either `gg` or `G` is a one-based absolute line.
        if matches!(
            motion.kind(),
            MotionKind::BufferTop | MotionKind::BufferBottom
        ) && motion.has_explicit_count()
        {
            motion =
                Motion::new(MotionKind::BufferTop).counted(motion.count(), CountSource::Explicit);
        }
        if matches!(
            motion.kind(),
            MotionKind::FindForward
                | MotionKind::FindBackward
                | MotionKind::TillForward
                | MotionKind::TillBackward
        ) {
            if motion.target().is_none() {
                self.awaiting_target = Some(motion);
                return MotionResolution::AwaitingTarget;
            }
            self.last_character_search = Some(motion);
        }
        if matches!(
            motion.kind(),
            MotionKind::RepeatCharacterSearch | MotionKind::ReverseCharacterSearch
        ) {
            let Some(mut repeated) = self.last_character_search else {
                return MotionResolution::Unavailable;
            };
            if motion.kind() == MotionKind::ReverseCharacterSearch {
                let Some(target) = repeated.target() else {
                    return MotionResolution::Unavailable;
                };
                let source = if motion.has_explicit_count() {
                    CountSource::Explicit
                } else {
                    CountSource::Implicit
                };
                repeated = Motion::new(reverse_character_search(repeated.kind()))
                    .counted(motion.count(), source)
                    .targeting(target);
            } else {
                let source = if motion.has_explicit_count() {
                    CountSource::Explicit
                } else {
                    CountSource::Implicit
                };
                repeated = repeated.counted(motion.count(), source);
            }
            return MotionResolution::Ready(repeated.repeating());
        }
        MotionResolution::Ready(motion)
    }

    /// Supplies the character argument for a pending find or till motion.
    ///
    /// Returns `None` when no motion is waiting. Any Unicode scalar is accepted;
    /// whether it exists in the current line is decided when the motion is
    /// applied. A completed motion becomes the source for `;` and `,`.
    pub fn accept_target(&mut self, target: char) -> Option<Motion> {
        let motion = self.awaiting_target.take()?.targeting(target);
        self.last_character_search = Some(motion);
        Some(motion)
    }

    /// Cancels only the pending count and character argument.
    ///
    /// The last completed character search remains available to `;` and `,`,
    /// matching Vim when an unrelated or cancelled command follows it.
    pub fn reset_pending(&mut self) {
        self.count = None;
        self.awaiting_target = None;
    }
}

fn reverse_character_search(kind: MotionKind) -> MotionKind {
    match kind {
        MotionKind::FindForward => MotionKind::FindBackward,
        MotionKind::FindBackward => MotionKind::FindForward,
        MotionKind::TillForward => MotionKind::TillBackward,
        MotionKind::TillBackward => MotionKind::TillForward,
        _ => kind,
    }
}

#[cfg(test)]
mod tests {
    use super::{MotionResolution, MotionState};
    use crate::{CountSource, Motion, MotionKind};

    #[test]
    fn a_pre_targeted_search_becomes_the_repeat_source() {
        let mut state = MotionState::new();
        let search = Motion::new(MotionKind::FindForward).targeting('界');
        assert_eq!(state.finish(search), MotionResolution::Ready(search));
        assert_eq!(
            state.finish(Motion::new(MotionKind::RepeatCharacterSearch)),
            MotionResolution::Ready(search.repeating())
        );
        assert_eq!(
            state.finish(Motion::new(MotionKind::ReverseCharacterSearch)),
            MotionResolution::Ready(
                Motion::new(MotionKind::FindBackward)
                    .targeting('界')
                    .repeating()
            )
        );
    }

    #[test]
    fn counts_saturate_and_zero_remains_a_motion_without_a_prefix() {
        let mut state = MotionState::new();
        assert!(!state.push_count_digit('0'));
        assert!(state.push_count_digit('1'));
        assert!(state.push_count_digit('2'));
        assert_eq!(
            state.finish(Motion::new(MotionKind::WordForward)),
            MotionResolution::Ready(
                Motion::new(MotionKind::WordForward).counted(12, CountSource::Explicit)
            )
        );
        for _ in 0..usize::BITS {
            assert!(state.push_count_digit('9'));
        }
        let MotionResolution::Ready(motion) = state.finish(Motion::new(MotionKind::Right)) else {
            panic!("counted motion should be ready");
        };
        assert_eq!(motion.count(), usize::MAX);
    }

    #[test]
    fn character_targets_repeat_and_survive_pending_resets() {
        let mut state = MotionState::new();
        assert_eq!(
            state.finish(Motion::new(MotionKind::TillForward)),
            MotionResolution::AwaitingTarget
        );
        assert_eq!(
            state.accept_target('x'),
            Some(Motion::new(MotionKind::TillForward).targeting('x'))
        );
        assert!(state.push_count_digit('2'));
        let MotionResolution::Ready(repeated) =
            state.finish(Motion::new(MotionKind::RepeatCharacterSearch))
        else {
            panic!("repeat should use the last character search");
        };
        assert_eq!(repeated.kind(), MotionKind::TillForward);
        assert_eq!(repeated.count(), 2);
        assert!(repeated.is_repeated());

        state.reset_pending();
        let MotionResolution::Ready(reversed) =
            state.finish(Motion::new(MotionKind::ReverseCharacterSearch))
        else {
            panic!("reverse repeat should survive pending resets");
        };
        assert_eq!(reversed.kind(), MotionKind::TillBackward);
        assert_eq!(reversed.target(), Some('x'));
    }

    #[test]
    fn counted_percent_and_g_use_vim_interpretations() {
        let mut state = MotionState::new();
        assert!(state.push_count_digit('5'));
        assert_eq!(
            state.finish(Motion::new(MotionKind::MatchingPair)),
            MotionResolution::Ready(
                Motion::new(MotionKind::BufferPercentage).counted(5, CountSource::Explicit)
            )
        );
        assert!(state.push_count_digit('4'));
        assert_eq!(
            state.finish(Motion::new(MotionKind::BufferBottom)),
            MotionResolution::Ready(
                Motion::new(MotionKind::BufferTop).counted(4, CountSource::Explicit)
            )
        );
    }
}
