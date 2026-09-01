//! Parsers for the stable, machine-oriented formats requested by [`GitCommand`].
//!
//! NUL-delimited records preserve repository path bytes. Parsers for structured
//! output reject incomplete records rather than accepting data truncated by the
//! runner's resource limits; patch parsing can instead return a truncated
//! document for display.
//!
//! [`GitCommand`]: crate::git::GitCommand

mod log;
mod name_status;
mod patch;
mod search;
mod status;
mod tree;

pub(crate) use log::parse_commits;
pub(crate) use name_status::parse_changed_files;
pub(crate) use patch::parse_patch;
pub(crate) use search::{parse_file_paths, parse_grep_matches};
pub(crate) use status::parse_status;
pub(crate) use tree::parse_tree_entries;
