//! Minimal standard-alphabet base64 decoder (portraits are stored base64 in character files).

/// Decode base64, ignoring ASCII whitespace; `None` on any invalid character or data after padding.
pub(crate) fn decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut pad = 0usize;
    for b in input.bytes() {
        if pad > 0 {
            match b {
                b'=' => {
                    pad += 1;
                    if pad > 2 {
                        return None;
                    }
                }
                b' ' | b'\n' | b'\r' | b'\t' => {}
                _ => return None,
            }
            continue;
        }
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                pad = 1;
                continue;
            }
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).ok()?);
        }
    }
    Some(out)
}

/// Encode bytes as standard padded base64.
pub(crate) fn encode(bytes: &[u8]) -> String {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHA[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_padded_and_rejects_garbage() {
        assert_eq!(decode("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(decode("aGV s\nbG8").as_deref(), Some(&b"hello"[..]));
        assert!(decode("a$b").is_none());
        assert!(decode("aGVsbG8=x").is_none());
        assert!(decode("iVBORw0KGgo=\" onload=\"alert(1)").is_none());
        assert!(decode("aGVsbG8===").is_none());
    }

    #[test]
    fn encode_round_trips() {
        for data in [&b""[..], b"a", b"ab", b"abc", b"hello world"] {
            assert_eq!(decode(&super::encode(data)).as_deref(), Some(data));
        }
        assert_eq!(super::encode(b"hello"), "aGVsbG8=");
    }
}
