use std::{
    fmt::{Debug, Display},
    str::Utf8Error,
};

/// What the parser was looking for when bytes ran out
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhileParsing {
    ItemHeader,
    HeaderValue,
    ArrayItem,
    DictItem,
    BytesFragment,
    BytesValue,
    StringFragment,
    StringValue,
}

/// Errors that may be encountered when parsing CBOR bytes
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// lower five bits of item header were > 27
    InvalidInfo,
    /// extra bytes were left while extracting the top-level item or decoding a TAG_CBOR_ITEM byte string
    TrailingGarbage,
    /// indefinite size encoding of (byte or text) strings requires definite size chunks to have the same major type
    InvalidStringFragment,
    /// a text string (or fragment thereof) contained invalid UTF-8 data
    InvalidUtf8(Utf8Error),
    /// the provided bytes are incomplete
    ///
    /// This error can be flagged also at the end of a TAG_CBOR_ITEM byte string, i.e.
    /// in the middle of the validated bytes.
    UnexpectedEof(WhileParsing),
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::InvalidInfo => write!(f, "invalid item header"),
            ErrorKind::TrailingGarbage => write!(f, "trailing garbage"),
            ErrorKind::InvalidStringFragment => write!(f, "string fragment of wrong major type"),
            ErrorKind::InvalidUtf8(e) => write!(f, "UTF-8 error `{}`", e),
            ErrorKind::UnexpectedEof(w) => write!(f, "ran out of bytes while parsing {:?}", w),
        }
    }
}

/// Error container for parsing problems
#[derive(Clone, PartialEq, Eq)]
pub struct ParseError {
    offset: usize,
    kind: ErrorKind,
}

impl ParseError {
    /// Get a reference to the parse error's offset.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Get a reference to the parse error's kind.
    pub fn kind(&self) -> ErrorKind {
        self.kind.clone()
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at offset {}", self.kind, self.offset)
    }
}

impl std::error::Error for ParseError {}

impl Debug for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

pub(crate) struct InternalError<'a> {
    position: &'a [u8],
    kind: ErrorKind,
}

impl<'a> InternalError<'a> {
    pub fn new(position: &'a [u8], kind: ErrorKind) -> Self {
        Self { position, kind }
    }

    pub fn offset(&self, base: &[u8]) -> usize {
        let position = self.position as *const _ as *const u8;
        let base = base as *const _ as *const u8;
        // safety: self.position is a subslice of base
        unsafe { position.offset_from(base) as usize }
    }

    pub fn with_location(self, loc: &[u8]) -> InternalError<'_> {
        InternalError {
            position: loc,
            kind: self.kind,
        }
    }

    pub fn rebase(self, base: &[u8]) -> ParseError {
        ParseError {
            offset: self.offset(base),
            kind: self.kind,
        }
    }
}
