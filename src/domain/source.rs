//! Source coordinates and repository-contained semantic navigation results.

use std::fmt::{self, Display, Formatter};

use crate::domain::{ObjectId, RepoPath};

/// The repository snapshot from which a complete source file is read.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FileRevision {
    /// The current working-tree file.
    WorkingTree,
    /// The file as stored by one immutable commit.
    Commit(ObjectId),
}

impl FileRevision {
    /// Returns a short display label suitable for a viewer title.
    #[must_use]
    pub fn display(&self) -> String {
        self.to_string()
    }
}

impl Display for FileRevision {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkingTree => formatter.write_str("working tree"),
            Self::Commit(commit) => write!(formatter, "commit {}", commit.short()),
        }
    }
}

/// A document-symbol category from the Language Server Protocol.
///
/// The named variants cover the standard LSP 3.17 values. [`Self::Other`]
/// preserves future server values without degrading the domain model to an
/// unvalidated display string.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DocumentSymbolKind {
    /// The server omitted the optional symbol kind.
    Unspecified,
    /// A file.
    File,
    /// A module.
    Module,
    /// A namespace.
    Namespace,
    /// A package.
    Package,
    /// A class.
    Class,
    /// A method.
    Method,
    /// A property.
    Property,
    /// A field.
    Field,
    /// A constructor.
    Constructor,
    /// An enumeration.
    Enum,
    /// An interface.
    Interface,
    /// A function.
    Function,
    /// A variable.
    Variable,
    /// A constant.
    Constant,
    /// A string value.
    String,
    /// A numeric value.
    Number,
    /// A Boolean value.
    Boolean,
    /// An array value.
    Array,
    /// An object value.
    Object,
    /// A key.
    Key,
    /// A null value.
    Null,
    /// An enumeration member.
    EnumMember,
    /// A structure.
    Struct,
    /// An event.
    Event,
    /// An operator.
    Operator,
    /// A type parameter.
    TypeParameter,
    /// A non-standard or newer LSP numeric kind.
    Other(u64),
}

impl DocumentSymbolKind {
    /// Returns the stable lowercase label used by the terminal UI.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Module => "module",
            Self::Namespace => "namespace",
            Self::Package => "package",
            Self::Class => "class",
            Self::Method => "method",
            Self::Property => "property",
            Self::Field => "field",
            Self::Constructor => "constructor",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Function => "function",
            Self::Variable => "variable",
            Self::Constant => "constant",
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Array => "array",
            Self::Object => "object",
            Self::Key => "key",
            Self::Null => "null",
            Self::EnumMember => "enum member",
            Self::Struct => "struct",
            Self::Event => "event",
            Self::Operator => "operator",
            Self::TypeParameter => "type parameter",
            Self::Unspecified | Self::Other(_) => "symbol",
        }
    }
}

impl From<u64> for DocumentSymbolKind {
    fn from(value: u64) -> Self {
        match value {
            1 => Self::File,
            2 => Self::Module,
            3 => Self::Namespace,
            4 => Self::Package,
            5 => Self::Class,
            6 => Self::Method,
            7 => Self::Property,
            8 => Self::Field,
            9 => Self::Constructor,
            10 => Self::Enum,
            11 => Self::Interface,
            12 => Self::Function,
            13 => Self::Variable,
            14 => Self::Constant,
            15 => Self::String,
            16 => Self::Number,
            17 => Self::Boolean,
            18 => Self::Array,
            19 => Self::Object,
            20 => Self::Key,
            21 => Self::Null,
            22 => Self::EnumMember,
            23 => Self::Struct,
            24 => Self::Event,
            25 => Self::Operator,
            26 => Self::TypeParameter,
            value => Self::Other(value),
        }
    }
}

impl Display for DocumentSymbolKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// One flattened document symbol returned by a language server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentSymbol {
    name: String,
    detail: Option<String>,
    kind: DocumentSymbolKind,
    range: SourceRange,
    selection: SourcePosition,
    depth: usize,
}

impl DocumentSymbol {
    /// Creates a display-ready symbol whose source coordinates use UTF-8 bytes.
    #[must_use]
    pub fn new(
        name: String,
        detail: Option<String>,
        kind: DocumentSymbolKind,
        range: SourceRange,
        selection: SourcePosition,
        depth: usize,
    ) -> Self {
        Self {
            name,
            detail,
            kind,
            range,
            selection,
            depth,
        }
    }

    /// Returns the server-provided symbol name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns optional signature or type detail.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Returns the normalized LSP symbol category.
    #[must_use]
    pub const fn kind(&self) -> DocumentSymbolKind {
        self.kind
    }

    /// Returns the complete source range occupied by the symbol.
    #[must_use]
    pub fn range(&self) -> SourceRange {
        self.range
    }

    /// Returns the preferred cursor position when the symbol is selected.
    #[must_use]
    pub fn selection(&self) -> SourcePosition {
        self.selection
    }

    /// Returns the nesting depth from the language-server response.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.depth
    }
}

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

/// A half-open source range expressed in `ChronoGit` coordinates.
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

    /// Splits the location into its repository-relative path and source range.
    #[must_use]
    pub fn into_parts(self) -> (RepoPath, SourceRange) {
        (self.path, self.selection)
    }
}

/// A normalized language-server navigation result.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum NavigationTarget {
    /// A regular file contained by the active repository.
    Repository(RepositoryLocation),
    /// A result `ChronoGit` deliberately refuses to open.
    External {
        /// Sanitized URI suitable for a short notice or result row.
        display_uri: String,
    },
}

impl NavigationTarget {
    /// Returns a short display label without interpreting an external URI as a path.
    #[must_use]
    pub fn display(&self) -> String {
        self.to_string()
    }
}

impl Display for NavigationTarget {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(location) => write!(
                formatter,
                "{}:{}:{}",
                location.path().display(),
                location.selection().start().line().saturating_add(1),
                location.selection().start().byte_column().saturating_add(1)
            ),
            Self::External { display_uri } => write!(formatter, "unsupported: {display_uri}"),
        }
    }
}
