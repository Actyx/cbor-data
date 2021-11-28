use crate::{ItemKind, TaggedItem};

/// Representation of a Timestamp
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    unix_epoch: i64,
    nanos: u32,
    tz_sec_east: i32,
}

impl Timestamp {
    pub fn new(unix_epoch: i64, nanos: u32, tz_sec_east: i32) -> Self {
        Self {
            unix_epoch,
            nanos,
            tz_sec_east,
        }
    }

    #[cfg(feature = "rfc3339")]
    pub(crate) fn from_string(item: TaggedItem<'_>) -> Option<Self> {
        if let ItemKind::Str(s) = item.kind() {
            chrono::DateTime::parse_from_rfc3339(s.as_cow().as_ref())
                .map(|dt| Timestamp {
                    unix_epoch: dt.timestamp(),
                    nanos: dt.timestamp_subsec_nanos(),
                    tz_sec_east: dt.offset().local_minus_utc(),
                })
                .ok()
        } else {
            None
        }
    }

    pub(crate) fn from_epoch(item: TaggedItem<'_>) -> Option<Self> {
        match item.kind() {
            ItemKind::Pos(t) => Some(Timestamp {
                unix_epoch: t.min(i64::MAX as u64) as i64,
                nanos: 0,
                tz_sec_east: 0,
            }),
            ItemKind::Neg(t) => Some(Timestamp {
                unix_epoch: -1 - t.min(i64::MAX as u64) as i64,
                nanos: 0,
                tz_sec_east: 0,
            }),
            ItemKind::Float(t) => Some(Timestamp {
                unix_epoch: t.min(i64::MAX as f64) as i64,
                nanos: ((t - t.floor()) * 1e9) as u32,
                tz_sec_east: 0,
            }),
            _ => None,
        }
    }

    /// timestamp value in seconds since the Unix epoch
    pub fn unix_epoch(&self) -> i64 {
        self.unix_epoch
    }

    /// fractional part in nanoseconds, to be added
    pub fn nanos(&self) -> u32 {
        self.nanos
    }

    /// timezone to use when encoding as a string, in seconds to the east
    pub fn tz_sec_east(&self) -> i32 {
        self.tz_sec_east
    }
}
