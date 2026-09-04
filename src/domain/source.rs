//! Source coordinates and repository-contained semantic navigation results.

use crate::domain::RepoPath;

/// A standard Language Server Protocol semantic navigation operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticNavigationKind {
    /// Find where the symbol is defined.
    Definition,
    /// Find concrete implementations of the symbol.
    Implementation,
    /// Find the definition of the symbol's type.
    TypeDefinition,
    /// Find the symbol's declaration.
    Declaration,
}

impl SemanticNavigationKind {
    /// Returns the short operation name used in notices and status text.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Implementation => "implementation",
            Self::TypeDefinition => "type definition",
            Self::Declaration => "declaration",
        }
    }
}

/// A source position using a zero-based line and UTF-8 byte column.
///
/// The byte column is always expected to lie on a UTF-8 character boundary.
/// Conversion to the language server's negotiated wire encoding happens only
/// inside the LSP adapter.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SourcePosition {
    line: u32,
    byte_column: usize,
}

impl SourcePosition {
    /// Creates a zero-based source position.
    #[must_use]
    pub const fn new(line: u32, byte_column: usize) -> Self {
        Self { line, byte_column }
    }

    /// Returns the zero-based source line.
    #[must_use]
    pub const fn line(self) -> u32 {
        self.line
    }

    /// Returns the zero-based UTF-8 byte column.
    #[must_use]
    pub const fn byte_column(self) -> usize {
        self.byte_column
    }
}

/// A half-open source range expressed in ChronoGit coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SourceRange {
    start: SourcePosition,
    end: SourcePosition,
}

impl SourceRange {
    /// Creates a half-open source range.
    #[must_use]
    pub const fn new(start: SourcePosition, end: SourcePosition) -> Self {
        Self { start, end }
    }

    /// Returns the inclusive start position.
    #[must_use]
    pub const fn start(self) -> SourcePosition {
        self.start
    }

    /// Returns the exclusive end position.
    #[must_use]
    pub const fn end(self) -> SourcePosition {
        self.end
    }
}

/// A semantic target that is safe to open through the repository-rooted reader.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RepositoryLocation {
    path: RepoPath,
    selection: SourceRange,
}

impl RepositoryLocation {
    /// Creates a repository-contained location.
    #[must_use]
    pub fn new(path: RepoPath, selection: SourceRange) -> Self {
        Self { path, selection }
    }

    /// Returns the repository-relative target path.
    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// Returns the server-selected source range.
    #[must_use]
    pub const fn selection(&self) -> SourceRange {
        self.selection
    }
}

/// A normalized language-server navigation result.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum NavigationTarget {
    /// A regular file contained by the active repository.
    Repository(RepositoryLocation),
    /// A result ChronoGit deliberately refuses to open.
    External {
        /// Sanitized URI suitable for a short notice or result row.
        display_uri: String,
    },
}

impl NavigationTarget {
    /// Returns a short display label without interpreting an external URI as a path.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Repository(location) => format!(
                "{}:{}:{}",
                location.path().display(),
                location.selection().start().line().saturating_add(1),
                location.selection().start().byte_column().saturating_add(1)
            ),
            Self::External { display_uri } => format!("unsupported: {display_uri}"),
        }
    }
}
