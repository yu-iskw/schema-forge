//! Source file loading and span tracking for Schemaforge.
//!
//! A [`SourceMap`] owns all loaded [`SourceFile`]s and assigns each a stable
//! [`SourceId`]. A [`Span`] is a half-open byte range `[start, end)` within
//! one source file, used throughout the compiler for error reporting.

use std::sync::Arc;

/// Opaque, copy-cheap identifier for a source file in a [`SourceMap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(u32);

impl SourceId {
    /// The numeric value of this ID.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source:{}", self.0)
    }
}

/// A half-open byte range `[start, end)` within a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// The source file this span belongs to.
    pub source: SourceId,
    /// Inclusive start byte offset.
    pub start: u32,
    /// Exclusive end byte offset.
    pub end: u32,
}

impl Span {
    /// Construct a new span.
    #[must_use]
    pub const fn new(source: SourceId, start: u32, end: u32) -> Self {
        Self { source, start, end }
    }

    /// Length of the span in bytes.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Returns `true` when the span covers zero bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// Merge two spans from the same source into their union.
    ///
    /// Returns `None` if the spans belong to different sources.
    #[must_use]
    pub fn merge(self, other: Self) -> Option<Self> {
        if self.source != other.source {
            return None;
        }
        Some(Self {
            source: self.source,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        })
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.source, self.start, self.end)
    }
}

/// A source file loaded into memory.
#[derive(Debug, Clone)]
pub struct SourceFile {
    id: SourceId,
    uri: String,
    content: Arc<str>,
    /// Byte offset of the start of each line (index 0 = line 1).
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Create a new source file with the given URI and content.
    #[must_use]
    pub fn new(id: SourceId, uri: impl Into<String>, content: impl Into<Arc<str>>) -> Self {
        let content = content.into();
        let line_starts = line_starts_for(&content);
        Self {
            id,
            uri: uri.into(),
            content,
            line_starts,
        }
    }

    /// The stable ID of this file.
    #[must_use]
    pub const fn id(&self) -> SourceId {
        self.id
    }

    /// The URI (or file path) identifying this source.
    #[must_use]
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// The full text content of this file.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Convert a byte offset into a 1-based `(line, column)` pair.
    #[must_use]
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line_idx = self
            .line_starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1);
        let line = u32::try_from(line_idx).unwrap_or(u32::MAX) + 1;
        let col = offset - self.line_starts[line_idx] + 1;
        (line, col)
    }

    /// Slice the text covered by `span`.
    #[must_use]
    pub fn span_text(&self, span: Span) -> &str {
        let s = span.start as usize;
        let e = (span.end as usize).min(self.content.len());
        &self.content[s..e]
    }
}

/// Compute the byte offset of the start of each line.
fn line_starts_for(text: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            let next = u32::try_from(i + 1).unwrap_or(u32::MAX);
            starts.push(next);
        }
    }
    starts
}

/// Registry that owns all loaded [`SourceFile`]s.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Create an empty source map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a new source file and return its assigned [`SourceId`].
    pub fn add(&mut self, uri: impl Into<String>, content: impl Into<Arc<str>>) -> SourceId {
        let index = u32::try_from(self.files.len()).unwrap_or(u32::MAX);
        let id = SourceId(index);
        self.files.push(SourceFile::new(id, uri, content));
        id
    }

    /// Look up a source file by ID.
    #[must_use]
    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    /// Number of loaded source files.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.files.len()
    }

    /// Returns `true` when no files have been loaded.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_map_add_and_get() {
        let mut map = SourceMap::new();
        let id = map.add("file:///schema.json", r#"{"type":"string"}"#);
        assert_eq!(id.index(), 0);
        let file = map.get(id).unwrap();
        assert_eq!(file.uri(), "file:///schema.json");
        assert_eq!(file.content(), r#"{"type":"string"}"#);
    }

    #[test]
    fn line_col_single_line() {
        let mut map = SourceMap::new();
        let id = map.add("test://a", "hello");
        let file = map.get(id).unwrap();
        assert_eq!(file.line_col(0), (1, 1));
        assert_eq!(file.line_col(4), (1, 5));
    }

    #[test]
    fn line_col_multi_line() {
        let mut map = SourceMap::new();
        let id = map.add("test://b", "ab\ncd\nef");
        let file = map.get(id).unwrap();
        assert_eq!(file.line_col(3), (2, 1));
        assert_eq!(file.line_col(6), (3, 1));
    }

    #[test]
    fn span_merge() {
        let id = SourceId(0);
        let a = Span::new(id, 0, 5);
        let b = Span::new(id, 3, 10);
        let merged = a.merge(b).unwrap();
        assert_eq!(merged.start, 0);
        assert_eq!(merged.end, 10);
    }

    #[test]
    fn span_merge_different_sources() {
        let a = Span::new(SourceId(0), 0, 5);
        let b = Span::new(SourceId(1), 3, 10);
        assert!(a.merge(b).is_none());
    }

    #[test]
    fn span_len_and_empty() {
        let id = SourceId(0);
        let s = Span::new(id, 3, 7);
        assert_eq!(s.len(), 4);
        assert!(!s.is_empty());
        let empty = Span::new(id, 5, 5);
        assert!(empty.is_empty());
    }
}
