//! The largest single block decoding a JSON client request asks the heap
//! for, read off the request's shape before it is decoded: what the request
//! gate ([`super::server_payload::request_refusal`]) holds the heap's largest
//! free block against.
//!
//! The gate used to assume the request is one base64 blob (a file write) and
//! asked for a block of 3/4 of the whole message. Studio's shader edit is not
//! one: an overlay mutation's `ReplaceBody` carries the shader as a JSON
//! array of byte values, ~3.6 characters per byte, which decodes into a
//! `Vec<u8>` of one byte per element, grown by doubling. A 7,142 B edit of
//! a 1,971 B shader needs a 2,048 B block, and the gate asked for 6,380 B, so
//! a fragmented board refused an edit it could decode
//! (`docs/defects/2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md`).
//!
//! The estimate is the largest of what each value builds:
//!
//! - **a string** of `L` characters: 3/4 of `L`, the blob rule the gate had
//!   (a file write's text or base64 is the request's one long string, so
//!   that request asks for what it asked for before);
//! - **an array of byte values** (`0..=255`) of `k` elements: a `Vec<u8>`
//!   grown by doubling to `k`, the next power of two;
//! - **any other array**: its own `Vec` of up to 64 B an element (what the
//!   elements own is counted as their own values), and never more than 3/4
//!   of its text, the blob rule on its span.
//!
//! It never exceeds the old rule's 3/4 of the whole message: the change only
//! lowers the block for a request whose bulk is not one string. A request
//! nested deeper than the scanner follows is measured by the old rule.

/// Containers the scan follows; deeper requests are measured whole.
const MAX_DEPTH: usize = 32;

/// The most one element of an array that is not byte values takes in its
/// `Vec` (a request's structs, strings' headers, numbers), beside what it
/// owns, which is counted as its own value.
const ELEMENT_BYTES: usize = 64;

/// The largest single block decoding `data` (a JSON client message) asks
/// for, by its shape; never more than 3/4 of `data`.
pub fn request_decode_block(data: &[u8]) -> usize {
    let whole = data.len() * 3 / 4;
    shape_block(data).map_or(whole, |block| block.min(whole))
}

/// One open array or object.
#[derive(Clone, Copy)]
struct Frame {
    array: bool,
    start: usize,
    elements: usize,
    all_bytes: bool,
}

/// [`request_decode_block`] by shape; `None` if the request nests deeper
/// than [`MAX_DEPTH`].
fn shape_block(data: &[u8]) -> Option<usize> {
    let mut stack = [Frame {
        array: false,
        start: 0,
        elements: 0,
        all_bytes: true,
    }; MAX_DEPTH];
    let mut depth = 0usize;
    let mut largest = 0usize;
    let mut i = 0usize;
    while i < data.len() {
        let byte = data[i];
        match byte {
            b'"' => {
                let start = i + 1;
                i = start;
                while i < data.len() && data[i] != b'"' {
                    i += if data[i] == b'\\' { 2 } else { 1 };
                }
                let len = i.min(data.len()) - start;
                largest = largest.max(len * 3 / 4);
                note_element(&mut stack, depth, false);
            }
            b'[' | b'{' => {
                note_element(&mut stack, depth, false);
                if depth == MAX_DEPTH {
                    return None;
                }
                stack[depth] = Frame {
                    array: byte == b'[',
                    start: i,
                    elements: 0,
                    all_bytes: true,
                };
                depth += 1;
            }
            b']' | b'}' => {
                if depth > 0 {
                    depth -= 1;
                    let frame = stack[depth];
                    if frame.array {
                        let grown = frame.elements.next_power_of_two().max(8);
                        let block = if frame.elements > 0 && frame.all_bytes {
                            grown
                        } else {
                            // The array's own `Vec`, its elements' contents
                            // being their own blocks (counted as they
                            // close): at most `ELEMENT_BYTES` each, and never
                            // more than the blob rule on its text.
                            (grown * ELEMENT_BYTES).min((i + 1 - frame.start) * 3 / 4)
                        };
                        largest = largest.max(block);
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                while i + 1 < data.len()
                    && matches!(data[i + 1], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                let token = &data[start..=i];
                let is_byte = token.len() <= 3
                    && token.iter().all(u8::is_ascii_digit)
                    && token.iter().fold(0u16, |n, d| n * 10 + u16::from(d - b'0')) <= 255;
                note_element(&mut stack, depth, is_byte);
            }
            b't' | b'f' | b'n' => {
                // `true`, `false`, `null`: one element, not a byte.
                while i + 1 < data.len() && data[i + 1].is_ascii_alphabetic() {
                    i += 1;
                }
                note_element(&mut stack, depth, false);
            }
            _ => {}
        }
        i += 1;
    }
    Some(largest)
}

/// A value starts inside the innermost open container: count it, and note
/// whether it is a byte value.
fn note_element(stack: &mut [Frame; MAX_DEPTH], depth: usize, is_byte: bool) {
    if depth == 0 {
        return;
    }
    let frame = &mut stack[depth - 1];
    if frame.array {
        frame.elements += 1;
        frame.all_bytes &= is_byte;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::String;

    /// Studio's shader edit, as it goes on the wire: the 1,971 B choker
    /// shader as a byte array. It needs a 2,048 B block, not 3/4 of its
    /// ~7 KB.
    #[test]
    fn a_byte_array_edit_needs_its_byte_count_rounded_up_not_three_quarters_of_its_text() {
        let message = replace_body_edit(&shader(1_971));
        assert!(message.len() > 7_000, "{}", message.len());
        assert_eq!(request_decode_block(message.as_bytes()), 2_048);
    }

    /// A file write is one long string: the old rule, unchanged.
    #[test]
    fn a_file_write_still_needs_three_quarters_of_its_blob() {
        let mut write =
            String::from(r#"{"id":4242,"msg":{"filesystem":{"write":{"path":"/a","data":""#);
        write.push_str(&"QUFB".repeat(3_000));
        write.push_str(r#""}}}}"#);
        assert_eq!(request_decode_block(write.as_bytes()), 12_000 * 3 / 4);
    }

    /// Arrays of other values, nesting, escapes and the cap.
    #[test]
    fn other_values_take_the_blob_rule_on_their_own_span() {
        // Numbers that are not bytes: 3/4 of the array's text, at most.
        let floats = format!("{{\"id\":1,\"v\":[{}]}}", ["0.25"; 400].join(","));
        let span = 400 * 5 - 1 + 2;
        assert_eq!(request_decode_block(floats.as_bytes()), span * 3 / 4);
        // A short array of objects: its own Vec, not the objects' text.
        let objects = format!(
            "{{\"id\":1,\"v\":[{{\"b\":[{}]}}]}}",
            ["7"; 1_000].join(",")
        );
        assert_eq!(request_decode_block(objects.as_bytes()), 1_024);
        // A 256 is not a byte.
        assert_eq!(
            request_decode_block(br#"{"id":1,"v":[1,2,256]}"#),
            "[1,2,256]".len() * 3 / 4
        );
        // An escaped quote does not end a string.
        assert_eq!(
            request_decode_block(br#"{"id":1,"s":"ab\"cdefgh"}"#),
            "ab\\\"cdefgh".len() * 3 / 4
        );
        // Never more than 3/4 of the message.
        let message = br#"[1,2,3,4,5,6,7,8,9]"#;
        assert_eq!(request_decode_block(message), message.len() * 3 / 4);
        // Too deep to follow: the whole message.
        let deep = format!("{}{}", "[".repeat(40), "]".repeat(40));
        assert_eq!(request_decode_block(deep.as_bytes()), deep.len() * 3 / 4);
    }

    /// A shader-like body of `len` bytes.
    fn shader(len: usize) -> String {
        "vec3 color = texture(palette, vec2(t, 0.5)).rgb;\n"
            .chars()
            .cycle()
            .take(len)
            .collect()
    }

    /// Studio's `SetArtifactBody`/`ReplaceBody` mutation of `body`, as its
    /// JSON goes on the wire.
    fn replace_body_edit(body: &str) -> String {
        let bytes: alloc::vec::Vec<String> = body.bytes().map(|b| format!("{b}")).collect();
        format!(
            r#"{{"id":31,"msg":{{"projectCommand":{{"handle":1,"command":{{"mutateOverlay":{{"request":{{"batch":{{"commands":[{{"id":7,"mutation":{{"set_artifact_body":{{"artifact":{{"path":"/shader.glsl"}},"edit":{{"replace_body":[{}]}}}}}}}}]}}}}}}}}}}}}}}"#,
            bytes.join(",")
        )
    }
}
