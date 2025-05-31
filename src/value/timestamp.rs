use crate::{constants::TAG_EPOCH, Encoder, ItemKind, Literal, TaggedItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Precision {
    Seconds,
    Millis,
    Micros,
    Nanos,
}

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
                .map(Into::into)
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
            ItemKind::Float(t) => {
                let mut frac = t.fract();
                if frac < 0.0 {
                    frac += 1.0;
                }
                let mut unix_epoch = t.min(i64::MAX as f64).floor() as i64;

                let mut nanos = if t.abs() > 1e-3 * (1u64 << 52) as f64 {
                    (frac * 1e3).round() as u32 * 1_000_000
                } else if t.abs() > 1e-6 * (1u64 << 52) as f64 {
                    (frac * 1e6).round() as u32 * 1_000
                } else {
                    (frac * 1e9).round() as u32
                };
                if nanos > 999_999_999 {
                    nanos -= 1_000_000_000;
                    unix_epoch += 1;
                }

                Some(Timestamp {
                    unix_epoch,
                    nanos,
                    tz_sec_east: 0,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn encode<E: Encoder>(self, encoder: E, precision: Precision) -> E::Output {
        if precision == Precision::Seconds {
            if self.unix_epoch() >= 0 {
                encoder.write_pos(self.unix_epoch() as u64, [TAG_EPOCH])
            } else {
                encoder.write_neg((-1 - self.unix_epoch()) as u64, [TAG_EPOCH])
            }
        } else {
            #[cfg(feature = "rfc3339")]
            {
                use crate::constants::TAG_ISO8601;
                use chrono::{DateTime, FixedOffset};
                use std::convert::TryFrom;

                let mut this = self;
                let as_epoch = match precision {
                    Precision::Seconds => unreachable!(),
                    Precision::Millis => {
                        this.nanos -= this.nanos % 1_000_000;
                        this.unix_epoch().abs() <= (1 << 52) / 1000
                    }
                    Precision::Micros => {
                        this.nanos -= this.nanos % 1_000;
                        this.unix_epoch().abs() <= (1 << 52) / 1_000_000
                    }
                    Precision::Nanos => this.unix_epoch().abs() <= (1 << 52) / 1_000_000_000,
                };
                if let (false, Ok(dt)) = (as_epoch, DateTime::<FixedOffset>::try_from(this)) {
                    let s = dt.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
                    encoder.write_str(s.as_str(), [TAG_ISO8601])
                } else {
                    let v = this.unix_epoch() as f64 + this.nanos() as f64 / 1e9;
                    encoder.write_lit(Literal::L8(v.to_bits()), [TAG_EPOCH])
                }
            }
            #[cfg(not(feature = "rfc3339"))]
            {
                let mut this = self;
                match precision {
                    Precision::Millis => this.nanos -= this.nanos % 1_000_000,
                    Precision::Micros => this.nanos -= this.nanos % 1_000,
                    _ => {}
                };
                let v = this.unix_epoch() as f64 + this.nanos() as f64 / 1e9;
                encoder.write_lit(Literal::L8(v.to_bits()), [TAG_EPOCH])
            }
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

#[cfg(feature = "rfc3339")]
mod rfc3339 {
    use super::Timestamp;
    use chrono::{DateTime, FixedOffset, Offset, TimeZone, Utc};
    use std::convert::TryFrom;

    impl TryFrom<Timestamp> for DateTime<FixedOffset> {
        type Error = ();

        fn try_from(t: Timestamp) -> Result<Self, Self::Error> {
            Ok(FixedOffset::east_opt(t.tz_sec_east())
                .ok_or(())?
                .from_utc_datetime(
                    &DateTime::<Utc>::from_timestamp(t.unix_epoch(), t.nanos())
                        .ok_or(())?
                        .naive_utc(),
                ))
        }
    }

    impl<Tz: TimeZone> From<DateTime<Tz>> for Timestamp {
        fn from(dt: DateTime<Tz>) -> Self {
            Timestamp {
                unix_epoch: dt.timestamp(),
                nanos: dt.timestamp_subsec_nanos(),
                tz_sec_east: dt.offset().fix().local_minus_utc(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Precision::{self, *},
        Timestamp,
    };
    use crate::{constants::TAG_EPOCH, CborBuilder, Literal, Writer};

    #[test]
    fn encode() {
        fn e(t: i64, n: u32, tz: i32, p: Precision) -> String {
            let t = Timestamp {
                unix_epoch: t,
                nanos: n,
                tz_sec_east: tz,
            };
            t.encode(CborBuilder::new(), p).to_string()
        }

        assert_eq!(e(0, 0, 0, Seconds), "1(0)");
        assert_eq!(e(0, 900_000_000, 0, Seconds), "1(0)");
        assert_eq!(e(-1, 0, 0, Seconds), "1(-1)");
        assert_eq!(e(-1, 900_000_000, 0, Seconds), "1(-1)");
        assert_eq!(e(i64::MAX, 0, 0, Seconds), "1(9223372036854775807)");
        assert_eq!(e(i64::MIN, 0, 0, Seconds), "1(-9223372036854775808)");

        assert_eq!(e(0, 500_000_000, 0, Millis), "1(0.5)");
        #[cfg(feature = "rfc3339")]
        {
            assert_eq!(
                e((1 << 52) / 1_000_000, 123_456_789, 0, Micros),
                "1(4503599627.123456)"
            );
            assert_eq!(
                e((1 << 52) / 1_000_000 + 1, 123_456_789, 2700, Micros),
                "0(\"2112-09-18T00:38:48.123456+00:45\")"
            );
            assert_eq!(
                e((1 << 52) / 1_000_000 + 1, 123_000_000, 2700, Micros),
                "0(\"2112-09-18T00:38:48.123+00:45\")"
            );
            assert_eq!(
                e((1 << 52) / 1_000_000_000, 123_456_789, 0, Nanos),
                "1(4503599.123456789)"
            );
            assert_eq!(
                e((1 << 52) / 1_000_000_000 + 1, 123_456_789, -1800, Nanos),
                "0(\"1970-02-22T02:30:00.123456789-00:30\")"
            );
            assert_eq!(
                e((1 << 52) / 1_000_000_000 + 1, 123_000_000, -1800, Nanos),
                "0(\"1970-02-22T02:30:00.123-00:30\")"
            );
        }
        #[cfg(not(feature = "rfc3339"))]
        {
            assert_eq!(
                e((1 << 52) / 1_000_000, 123_456_789, 0, Micros),
                "1(4503599627.123456)"
            );
            assert_eq!(
                e((1 << 52) / 1_000_000 + 1, 123_456_789, 2700, Micros),
                "1(4503599628.123456)"
            );
            assert_eq!(
                e((1 << 52) / 1_000_000_000, 123_456_789, 0, Nanos),
                "1(4503599.123456789)"
            );
            assert_eq!(
                e((1 << 52) / 1_000_000_000 + 1, 123_456_789, -1800, Nanos),
                "1(4503600.123456789)"
            );
        }
    }

    #[test]
    #[cfg(feature = "rfc3339")]
    fn string() {
        use crate::{constants::TAG_ISO8601, Writer};

        let cbor = CborBuilder::new().write_str("2020-07-12T02:14:00.43-04:40", [TAG_ISO8601]);
        assert_eq!(
            Timestamp::from_string(cbor.tagged_item()).unwrap(),
            Timestamp::new(1594536840, 430_000_000, -16800)
        );
    }

    #[test]
    fn epoch() {
        let cbor = CborBuilder::new().write_pos(1594536840, [TAG_EPOCH]);
        assert_eq!(
            Timestamp::from_epoch(cbor.tagged_item()).unwrap(),
            Timestamp::new(1594536840, 0, 0)
        );

        let cbor = CborBuilder::new().write_neg(1594536840, [TAG_EPOCH]);
        assert_eq!(
            Timestamp::from_epoch(cbor.tagged_item()).unwrap(),
            Timestamp::new(-1594536841, 0, 0)
        );

        let cbor =
            CborBuilder::new().write_lit(Literal::L8(1594536840.01_f64.to_bits()), [TAG_EPOCH]);
        assert_eq!(
            Timestamp::from_epoch(cbor.tagged_item()).unwrap(),
            Timestamp::new(1594536840, 9_999_990, 0) // f64 precision limit
        );

        let cbor =
            CborBuilder::new().write_lit(Literal::L8(15945368400.01_f64.to_bits()), [TAG_EPOCH]);
        assert_eq!(
            Timestamp::from_epoch(cbor.tagged_item()).unwrap(),
            Timestamp::new(15945368400, 10_000_000, 0) // ditching meaningless nanos here
        );

        let cbor =
            CborBuilder::new().write_lit(Literal::L8((-15945368400.01_f64).to_bits()), [TAG_EPOCH]);
        assert_eq!(
            Timestamp::from_epoch(cbor.tagged_item()).unwrap(),
            Timestamp::new(-15945368401, 990_000_000, 0)
        );
    }
}
