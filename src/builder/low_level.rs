use crate::{constants::*, Literal};

pub enum Bytes<'a> {
    Owned(Vec<u8>),
    Borrowed(&'a mut Vec<u8>),
}

impl<'a> Bytes<'a> {
    pub fn copy(&mut self) -> Bytes {
        Bytes::Borrowed(self.as_mut())
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            Bytes::Owned(b) => b.as_slice(),
            Bytes::Borrowed(b) => b.as_slice(),
        }
    }

    pub fn as_mut(&mut self) -> &mut Vec<u8> {
        match self {
            Bytes::Owned(b) => b,
            Bytes::Borrowed(b) => *b,
        }
    }
}

/// Tags are from outer to inner.
pub fn write_positive(bytes: &mut Vec<u8>, value: u64, tags: impl IntoIterator<Item = u64>) {
    write_tags(bytes, tags);
    write_info(bytes, MAJOR_POS, value);
}

/// Tags are from outer to inner.
pub fn write_neg(bytes: &mut Vec<u8>, value: u64, tags: impl IntoIterator<Item = u64>) {
    write_tags(bytes, tags);
    write_info(bytes, MAJOR_NEG, value);
}

/// Tags are from outer to inner.
pub fn write_str(bytes: &mut Vec<u8>, value: &str, tags: impl IntoIterator<Item = u64>) {
    write_tags(bytes, tags);
    write_info(bytes, MAJOR_STR, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

/// Tags are from outer to inner.
pub fn write_bytes(bytes: &mut Vec<u8>, value: &[u8], tags: impl IntoIterator<Item = u64>) {
    write_tags(bytes, tags);
    write_info(bytes, MAJOR_BYTES, value.len() as u64);
    bytes.extend_from_slice(value);
}

/// Tags are from outer to inner.
pub fn write_bool(bytes: &mut Vec<u8>, value: bool, tags: impl IntoIterator<Item = u64>) {
    write_tags(bytes, tags);
    write_info(
        bytes,
        MAJOR_LIT,
        if value {
            LIT_TRUE.into()
        } else {
            LIT_FALSE.into()
        },
    );
}

/// Tags are from outer to inner.
pub fn write_null(bytes: &mut Vec<u8>, tags: impl IntoIterator<Item = u64>) {
    write_tags(bytes, tags);
    write_info(bytes, MAJOR_LIT, LIT_NULL.into());
}

/// Tags are from outer to inner.
pub fn write_undefined(bytes: &mut Vec<u8>, tags: impl IntoIterator<Item = u64>) {
    write_tags(bytes, tags);
    write_info(bytes, MAJOR_LIT, LIT_UNDEFINED.into());
}

/// Tags are from outer to inner.
pub(crate) fn write_tags(bytes: &mut Vec<u8>, tags: impl IntoIterator<Item = u64>) {
    for tag in tags {
        write_info(bytes, MAJOR_TAG, tag);
    }
}

pub fn write_info(bytes: &mut Vec<u8>, major: u8, value: u64) -> usize {
    if value < 24 {
        bytes.push(major << 5 | (value as u8));
        1
    } else if value < 0x100 {
        bytes.push(major << 5 | 24);
        bytes.push(value as u8);
        2
    } else if value < 0x1_0000 {
        bytes.push(major << 5 | 25);
        bytes.push((value >> 8) as u8);
        bytes.push(value as u8);
        3
    } else if value < 0x1_0000_0000 {
        bytes.push(major << 5 | 26);
        bytes.push((value >> 24) as u8);
        bytes.push((value >> 16) as u8);
        bytes.push((value >> 8) as u8);
        bytes.push(value as u8);
        5
    } else {
        bytes.push(major << 5 | 27);
        bytes.push((value >> 56) as u8);
        bytes.push((value >> 48) as u8);
        bytes.push((value >> 40) as u8);
        bytes.push((value >> 32) as u8);
        bytes.push((value >> 24) as u8);
        bytes.push((value >> 16) as u8);
        bytes.push((value >> 8) as u8);
        bytes.push(value as u8);
        9
    }
}

pub fn write_lit(bytes: &mut Vec<u8>, value: Literal) {
    match value {
        Literal::L0(v) => bytes.push(MAJOR_LIT << 5 | v),
        Literal::L1(v) => {
            bytes.push(MAJOR_LIT << 5 | 24);
            bytes.push(v);
        }
        Literal::L2(v) => {
            bytes.push(MAJOR_LIT << 5 | 25);
            bytes.push((v >> 8) as u8);
            bytes.push(v as u8);
        }
        Literal::L4(v) => {
            bytes.push(MAJOR_LIT << 5 | 26);
            bytes.push((v >> 24) as u8);
            bytes.push((v >> 16) as u8);
            bytes.push((v >> 8) as u8);
            bytes.push(v as u8);
        }
        Literal::L8(v) => {
            bytes.push(MAJOR_LIT << 5 | 27);
            bytes.push((v >> 56) as u8);
            bytes.push((v >> 48) as u8);
            bytes.push((v >> 40) as u8);
            bytes.push((v >> 32) as u8);
            bytes.push((v >> 24) as u8);
            bytes.push((v >> 16) as u8);
            bytes.push((v >> 8) as u8);
            bytes.push(v as u8);
        }
    }
}

pub fn write_indefinite(bytes: &mut Vec<u8>, major: u8) {
    bytes.push(major << 5 | INDEFINITE_SIZE);
}

pub fn finish_array(count: u64, b: &mut Vec<u8>, pos: usize, major: u8, max_definite: Option<u64>) {
    if Some(count) > max_definite {
        // indefinite encoding saves bytes here
        b.push(STOP_BYTE);
    } else {
        // otherwise prefer definite encoding
        let end = b.len();
        // use main vector as scratch space, will clean up below
        let head_len = write_info(b, major, count);
        if head_len > 1 {
            // save header bytes onto stack
            let mut buf = [0u8; 9];
            let buf = &mut buf[0..head_len];
            buf.copy_from_slice(&b[end..]);
            // need to shift back the array contents to make room for longer header
            let to_move = pos + 1..end;
            let new_start = pos + head_len;
            b.copy_within(to_move, new_start);
            // now put the new header in place
            b[pos..new_start].copy_from_slice(buf);
        } else {
            b[pos] = b[end];
        }
        // written header included the `pos` byte, so clean up
        b.pop();
    }
}
