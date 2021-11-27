use std::{fmt::Display, str::Utf8Error};

/// Errors that may be encountered when parsing CBOR bytes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// lower five bits of item header were > 27
    InvalidInfo,
    /// extra bytes were left while extracting the top-level item
    TrailingGarbage,
    /// indefinite size encoding of (byte or text) strings requires definite size chunks to have the same major type
    InvalidStringFragment,
    /// a text string (or fragment thereof) contained invalid UTF-8 data
    InvalidUtf8(Utf8Error),
}

/// Error container for parsing problems
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error<'a> {
    /// The provided bytes are incomplete
    ///
    /// The contained string describes what was currently being parsed when bytes ran out.
    UnexpectedEof(&'static str),
    /// Internal — you’ll never see this
    AtSlice(&'a [u8], ErrorKind),
    /// The given error was found at the given offset in the input.
    AtOffset(usize, ErrorKind),
}

impl<'a> Error<'a> {
    pub(crate) fn offset(&self, base: &[u8]) -> Option<usize> {
        let s = match self {
            Error::UnexpectedEof(_) => return None,
            Error::AtSlice(s, _) => *s,
            Error::AtOffset(_, _) => return None,
        };
        Some(unsafe { (&s[0] as *const u8).offset_from(&base[0] as *const u8) } as usize)
    }

    pub(crate) fn with_location(self, loc: &[u8]) -> Error<'_> {
        use Error::*;
        match self {
            UnexpectedEof(s) => UnexpectedEof(s),
            AtSlice(_, e) => AtSlice(loc, e),
            AtOffset(o, e) => AtOffset(o, e),
        }
    }

    pub(crate) fn rebase(self, base: &[u8]) -> Error<'static> {
        use Error::*;
        match self {
            UnexpectedEof(s) => UnexpectedEof(s),
            AtSlice(s, e) => AtOffset(
                unsafe { (&s[0] as *const u8).offset_from(&base[0] as *const u8) } as usize,
                e,
            ),
            AtOffset(o, e) => AtOffset(o, e),
        }
    }
}

impl<'a> Display for Error<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl<'a> std::error::Error for Error<'a> {}
