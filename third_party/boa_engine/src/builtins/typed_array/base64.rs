//! The base64 and hex methods of `Uint8Array`.
//!
//! More information:
//!  - [ECMAScript reference][spec]
//!
//! [spec]: https://tc39.es/ecma262/#sec-uint8array-base64

use std::sync::atomic::Ordering;

use crate::{
    Context, JsArgs, JsError, JsObject, JsResult, JsString, JsValue,
    builtins::{
        array_buffer::utils::SliceRefMut,
        typed_array::{BuiltinTypedArray, TypedArray, TypedArrayKind, Uint8Array},
    },
    js_error, js_string,
};

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Alphabet {
    Base64,
    Base64Url,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LastChunkHandling {
    Loose,
    Strict,
    StopBeforePartial,
}

/// The `[[Read]]`, `[[Bytes]]` and `[[Error]]` record returned by `FromBase64` and `FromHex`.
struct Decoded {
    read: usize,
    bytes: Vec<u8>,
    error: Option<JsError>,
}

/// `GetOptionsObject ( options )`
fn get_options_object(options: &JsValue) -> JsResult<JsObject> {
    if options.is_undefined() {
        return Ok(JsObject::with_null_proto());
    }
    options
        .as_object()
        .ok_or_else(|| js_error!(TypeError: "options must be an object"))
}

/// Reads and checks the `alphabet` option.
fn get_alphabet(options: &JsObject, context: &mut Context) -> JsResult<Alphabet> {
    // a. Let alphabet be ? Get(opts, "alphabet").
    let alphabet = options.get(js_string!("alphabet"), context)?;
    // b. If alphabet is undefined, set alphabet to "base64".
    if alphabet.is_undefined() {
        return Ok(Alphabet::Base64);
    }
    // c. If alphabet is neither "base64" nor "base64url", throw a TypeError exception.
    match alphabet.as_string() {
        Some(s) if s == js_string!("base64") => Ok(Alphabet::Base64),
        Some(s) if s == js_string!("base64url") => Ok(Alphabet::Base64Url),
        _ => Err(js_error!(TypeError: "alphabet must be \"base64\" or \"base64url\"")),
    }
}

/// Reads and checks the `lastChunkHandling` option.
fn get_last_chunk_handling(
    options: &JsObject,
    context: &mut Context,
) -> JsResult<LastChunkHandling> {
    // a. Let lastChunkHandling be ? Get(opts, "lastChunkHandling").
    let handling = options.get(js_string!("lastChunkHandling"), context)?;
    // b. If lastChunkHandling is undefined, set lastChunkHandling to "loose".
    if handling.is_undefined() {
        return Ok(LastChunkHandling::Loose);
    }
    // c. If lastChunkHandling is not one of "loose", "strict", or "stop-before-partial", throw a TypeError exception.
    match handling.as_string() {
        Some(s) if s == js_string!("loose") => Ok(LastChunkHandling::Loose),
        Some(s) if s == js_string!("strict") => Ok(LastChunkHandling::Strict),
        Some(s) if s == js_string!("stop-before-partial") => {
            Ok(LastChunkHandling::StopBeforePartial)
        }
        _ => Err(js_error!(
            TypeError: "lastChunkHandling must be \"loose\", \"strict\" or \"stop-before-partial\""
        )),
    }
}

/// `ValidateUint8Array ( ta )`
fn validate_uint8_array(ta: &JsValue) -> JsResult<JsObject<TypedArray>> {
    // 1. Perform ? RequireInternalSlot(ta, [[TypedArrayName]]).
    // 2. If ta.[[TypedArrayName]] is not "Uint8Array", throw a TypeError exception.
    ta.as_object()
        .and_then(|o| o.downcast::<TypedArray>().ok())
        .filter(|ta| ta.borrow().data().kind() == TypedArrayKind::Uint8)
        .ok_or_else(|| js_error!(TypeError: "`this` is not a Uint8Array"))
}

/// `GetUint8ArrayBytes ( ta )`
fn get_uint8_array_bytes(ta: &JsObject<TypedArray>) -> JsResult<Vec<u8>> {
    let ta = ta.borrow();
    let ta = ta.data();
    // 1. Let buffer be ta.[[ViewedArrayBuffer]].
    let buffer = ta.viewed_array_buffer().as_buffer();
    // 2. Let taRecord be MakeTypedArrayWithBufferWitnessRecord(ta, seq-cst).
    // 3. If IsTypedArrayOutOfBounds(taRecord) is true, throw a TypeError exception.
    let Some(bytes) = buffer
        .bytes(Ordering::SeqCst)
        .filter(|bytes| !ta.is_out_of_bounds(bytes.len()))
    else {
        return Err(js_error!(TypeError: "typed array is outside the bounds of its buffer"));
    };
    // 4. Let len be TypedArrayLength(taRecord).
    // 5. Let byteOffset be ta.[[ByteOffset]].
    let len = ta.array_length(bytes.len()) as usize;
    let offset = ta.byte_offset() as usize;
    // 6-9. Read len bytes starting at byteOffset.
    Ok(bytes.subslice(offset..offset + len).to_vec())
}

/// `SetUint8ArrayBytes ( into, bytes )`
fn set_uint8_array_bytes(into: &JsObject<TypedArray>, bytes: &[u8]) {
    let into = into.borrow();
    let into = into.data();
    let offset = into.byte_offset() as usize;
    let mut buffer = into.viewed_array_buffer().as_buffer_mut();
    let Some(mut data) = buffer.bytes(Ordering::SeqCst) else {
        return;
    };
    match data.subslice_mut(offset..offset + bytes.len()) {
        SliceRefMut::Slice(target) => target.copy_from_slice(bytes),
        SliceRefMut::AtomicSlice(target) => {
            for (target, byte) in target.iter().zip(bytes) {
                target.store(*byte, Ordering::SeqCst);
            }
        }
    }
}

/// The `TypedArrayLength` of a `Uint8Array` that is not out of bounds.
fn writable_length(into: &JsObject<TypedArray>) -> JsResult<usize> {
    let into = into.borrow();
    let into = into.data();
    let buffer = into.viewed_array_buffer().as_buffer();
    // 10. Let taRecord be MakeTypedArrayWithBufferWitnessRecord(into, seq-cst).
    // 11. If IsTypedArrayOutOfBounds(taRecord) is true, throw a TypeError exception.
    let Some(bytes) = buffer
        .bytes(Ordering::SeqCst)
        .filter(|bytes| !into.is_out_of_bounds(bytes.len()))
    else {
        return Err(js_error!(TypeError: "typed array is outside the bounds of its buffer"));
    };
    // 12. Let byteLength be TypedArrayLength(taRecord).
    Ok(into.array_length(bytes.len()) as usize)
}

/// Creates a new `Uint8Array` holding `bytes`.
fn new_uint8_array(bytes: &[u8], context: &mut Context) -> JsResult<JsValue> {
    let constructor = context
        .intrinsics()
        .constructors()
        .typed_uint8_array()
        .constructor();
    let ta = BuiltinTypedArray::allocate::<Uint8Array>(
        &constructor.into(),
        bytes.len() as u64,
        context,
    )?;
    let ta = ta
        .downcast::<TypedArray>()
        .map_err(|_| js_error!(TypeError: "Uint8Array allocation failed"))?;
    set_uint8_array_bytes(&ta, bytes);
    Ok(ta.upcast().into())
}

/// The `{ read, written }` result object of `setFromBase64` and `setFromHex`.
fn read_written(read: usize, written: usize, context: &mut Context) -> JsValue {
    let result = JsObject::with_object_proto(context.intrinsics());
    result
        .create_data_property_or_throw(js_string!("read"), read, context)
        .expect("CreateDataPropertyOrThrow cannot fail on a new object");
    result
        .create_data_property_or_throw(js_string!("written"), written, context)
        .expect("CreateDataPropertyOrThrow cannot fail on a new object");
    result.into()
}

/// `SkipAsciiWhitespace ( string, index )`
fn skip_ascii_whitespace(string: &[u16], mut index: usize) -> usize {
    while index < string.len() && matches!(string[index], 0x09 | 0x0A | 0x0C | 0x0D | 0x20) {
        index += 1;
    }
    index
}

fn base64_value(c: u8) -> u32 {
    match c {
        b'A'..=b'Z' => u32::from(c - b'A'),
        b'a'..=b'z' => u32::from(c - b'a') + 26,
        b'0'..=b'9' => u32::from(c - b'0') + 52,
        b'+' => 62,
        _ => 63,
    }
}

/// `DecodeBase64Chunk ( chunk [ , throwOnExtraBits ] )`
fn decode_base64_chunk(chunk: &[u8], throw_on_extra_bits: bool) -> JsResult<Vec<u8>> {
    // 1-4. Pad the chunk to four characters with "A".
    let mut n = 0u32;
    for i in 0..4 {
        n = (n << 6) | chunk.get(i).map_or(0, |&c| base64_value(c));
    }
    // 5. Let byteSequence be the unique sequence of 3 bytes resulting from decoding chunk as base64.
    let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
    match chunk.len() {
        // 7. If chunkLength = 2, then
        2 => {
            // b. If throwOnExtraBits is true and bytes[1] ≠ 0, then throw a SyntaxError exception.
            if throw_on_extra_bits && bytes[1] != 0 {
                return Err(js_error!(SyntaxError: "base64 chunk has non-zero padding bits"));
            }
            // c. Return « bytes[0] ».
            Ok(vec![bytes[0]])
        }
        // 8. Else if chunkLength = 3, then
        3 => {
            // b. If throwOnExtraBits is true and bytes[2] ≠ 0, then throw a SyntaxError exception.
            if throw_on_extra_bits && bytes[2] != 0 {
                return Err(js_error!(SyntaxError: "base64 chunk has non-zero padding bits"));
            }
            // c. Return « bytes[0], bytes[1] ».
            Ok(vec![bytes[0], bytes[1]])
        }
        // 9. Else, return bytes.
        _ => Ok(bytes.to_vec()),
    }
}

/// `FromBase64 ( string, alphabet, lastChunkHandling [ , maxLength ] )`
#[allow(clippy::too_many_lines)]
fn from_base64(
    string: &[u16],
    alphabet: Alphabet,
    last_chunk_handling: LastChunkHandling,
    max_length: usize,
) -> Decoded {
    let syntax_error = || Some(js_error!(SyntaxError: "invalid base64 string"));

    // 3. If maxLength = 0, then
    if max_length == 0 {
        // a. Return the Record { [[Read]]: 0, [[Bytes]]: « », [[Error]]: none }.
        return Decoded {
            read: 0,
            bytes: Vec::new(),
            error: None,
        };
    }

    // 4. Let read be 0.
    let mut read = 0;
    // 5. Let bytes be « ».
    let mut bytes = Vec::new();
    // 6. Let chunk be the empty String.
    // 7. Let chunkLength be 0.
    let mut chunk: Vec<u8> = Vec::with_capacity(4);
    // 8. Let index be 0.
    let mut index = 0;
    // 9. Let length be the length of string.
    let length = string.len();

    // 10. Repeat,
    loop {
        // a. Set index to SkipAsciiWhitespace(string, index).
        index = skip_ascii_whitespace(string, index);

        // b. If index = length, then
        if index == length {
            // i. If chunkLength > 0, then
            if !chunk.is_empty() {
                match last_chunk_handling {
                    // 1. If lastChunkHandling is "stop-before-partial", then
                    LastChunkHandling::StopBeforePartial => {
                        // a. Return the Record { [[Read]]: read, [[Bytes]]: bytes, [[Error]]: none }.
                        return Decoded {
                            read,
                            bytes,
                            error: None,
                        };
                    }
                    // 2. Else if lastChunkHandling is "loose", then
                    LastChunkHandling::Loose => {
                        // a. If chunkLength = 1, then
                        if chunk.len() == 1 {
                            // i. Let error be a new SyntaxError exception.
                            return Decoded {
                                read,
                                bytes,
                                error: syntax_error(),
                            };
                        }
                        // b. Set bytes to the list-concatenation of bytes and ! DecodeBase64Chunk(chunk, false).
                        bytes.extend(
                            decode_base64_chunk(&chunk, false).expect("cannot fail without checks"),
                        );
                    }
                    // 3. Else,
                    LastChunkHandling::Strict => {
                        // b. Let error be a new SyntaxError exception.
                        return Decoded {
                            read,
                            bytes,
                            error: syntax_error(),
                        };
                    }
                }
            }
            // ii. Return the Record { [[Read]]: length, [[Bytes]]: bytes, [[Error]]: none }.
            return Decoded {
                read: length,
                bytes,
                error: None,
            };
        }

        // c. Let char be the substring of string from index to index + 1.
        let mut char = string[index];
        // d. Set index to index + 1.
        index += 1;

        // e. If char is "=", then
        if char == u16::from(b'=') {
            // i. If chunkLength < 2, then
            if chunk.len() < 2 {
                return Decoded {
                    read,
                    bytes,
                    error: syntax_error(),
                };
            }
            // ii. Set index to SkipAsciiWhitespace(string, index).
            index = skip_ascii_whitespace(string, index);
            // iii. If chunkLength = 2, then
            if chunk.len() == 2 {
                // 1. If index = length, then
                if index == length {
                    // a. If lastChunkHandling is "stop-before-partial", then
                    if last_chunk_handling == LastChunkHandling::StopBeforePartial {
                        // i. Return the Record { [[Read]]: read, [[Bytes]]: bytes, [[Error]]: none }.
                        return Decoded {
                            read,
                            bytes,
                            error: None,
                        };
                    }
                    // b. Let error be a new SyntaxError exception.
                    return Decoded {
                        read,
                        bytes,
                        error: syntax_error(),
                    };
                }
                // 2. Set char to the substring of string from index to index + 1.
                // 3. If char is "=", then
                if string[index] == u16::from(b'=') {
                    // a. Set index to SkipAsciiWhitespace(string, index + 1).
                    index = skip_ascii_whitespace(string, index + 1);
                }
            }
            // iv. If index < length, then
            if index < length {
                return Decoded {
                    read,
                    bytes,
                    error: syntax_error(),
                };
            }
            // v. If lastChunkHandling is "strict", let throwOnExtraBits be true.
            // vi. Else, let throwOnExtraBits be false.
            let throw_on_extra_bits = last_chunk_handling == LastChunkHandling::Strict;
            // vii. Let decodeResult be DecodeBase64Chunk(chunk, throwOnExtraBits).
            match decode_base64_chunk(&chunk, throw_on_extra_bits) {
                // viii. If decodeResult is an abrupt completion, then
                Err(error) => {
                    return Decoded {
                        read,
                        bytes,
                        error: Some(error),
                    };
                }
                // ix. Set bytes to the list-concatenation of bytes and ! decodeResult.
                Ok(decoded) => bytes.extend(decoded),
            }
            // x. Return the Record { [[Read]]: length, [[Bytes]]: bytes, [[Error]]: none }.
            return Decoded {
                read: length,
                bytes,
                error: None,
            };
        }

        // f. If alphabet is "base64url", then
        if alphabet == Alphabet::Base64Url {
            // i. If char is either "+" or "/", then
            if char == u16::from(b'+') || char == u16::from(b'/') {
                return Decoded {
                    read,
                    bytes,
                    error: syntax_error(),
                };
            } else if char == u16::from(b'-') {
                // ii. Else if char is "-", then set char to "+".
                char = u16::from(b'+');
            } else if char == u16::from(b'_') {
                // iii. Else if char is "_", then set char to "/".
                char = u16::from(b'/');
            }
        }

        // g. If the sole code unit of char is not an element of the standard base64 alphabet, then
        let Some(char) = u8::try_from(char)
            .ok()
            .filter(|c| BASE64_ALPHABET.contains(c))
        else {
            return Decoded {
                read,
                bytes,
                error: syntax_error(),
            };
        };

        // h. Let remaining be maxLength - the length of bytes.
        let remaining = max_length - bytes.len();
        // i. If remaining = 1 and chunkLength = 2, or if remaining = 2 and chunkLength = 3, then
        if (remaining == 1 && chunk.len() == 2) || (remaining == 2 && chunk.len() == 3) {
            // i. Return the Record { [[Read]]: read, [[Bytes]]: bytes, [[Error]]: none }.
            return Decoded {
                read,
                bytes,
                error: None,
            };
        }

        // j. Set chunk to the string-concatenation of chunk and char.
        // k. Set chunkLength to the length of chunk.
        chunk.push(char);

        // l. If chunkLength = 4, then
        if chunk.len() == 4 {
            // i. Set bytes to the list-concatenation of bytes and ! DecodeBase64Chunk(chunk).
            bytes.extend(decode_base64_chunk(&chunk, false).expect("a full chunk cannot fail"));
            // ii. Set chunk to the empty String.
            // iii. Set chunkLength to 0.
            chunk.clear();
            // iv. Set read to index.
            read = index;
            // v. If the length of bytes = maxLength, then
            if bytes.len() == max_length {
                // 1. Return the Record { [[Read]]: read, [[Bytes]]: bytes, [[Error]]: none }.
                return Decoded {
                    read,
                    bytes,
                    error: None,
                };
            }
        }
    }
}

/// `FromHex ( string [ , maxLength ] )`
fn from_hex(string: &[u16], max_length: usize) -> Decoded {
    // 2. Let length be the length of string.
    let length = string.len();
    // 3. Let bytes be « ».
    let mut bytes = Vec::new();
    // 4. Let read be 0.
    let mut read = 0;
    // 5. If length modulo 2 is not 0, then
    if length % 2 != 0 {
        // a. Let error be a new SyntaxError exception.
        // b. Return the Record { [[Read]]: read, [[Bytes]]: bytes, [[Error]]: error }.
        return Decoded {
            read,
            bytes,
            error: Some(js_error!(SyntaxError: "hex string must have an even length")),
        };
    }
    let hexit = |c: u16| char::from_u32(u32::from(c)).and_then(|c| c.to_digit(16));
    // 6. Repeat, while read < length and the length of bytes < maxLength,
    while read < length && bytes.len() < max_length {
        // a. Let hexits be the substring of string from read to read + 2.
        // b. If hexits does not consist exclusively of hexadecimal ASCII digits, then
        let (Some(high), Some(low)) = (hexit(string[read]), hexit(string[read + 1])) else {
            // i. Let error be a new SyntaxError exception.
            // ii. Return the Record { [[Read]]: read, [[Bytes]]: bytes, [[Error]]: error }.
            return Decoded {
                read,
                bytes,
                error: Some(js_error!(SyntaxError: "invalid hex string")),
            };
        };
        // c. Set read to read + 2.
        read += 2;
        // d. Let byte be the integer value represented by hexits in base-16 notation.
        // e. Append byte to bytes.
        bytes.push((high * 16 + low) as u8);
    }
    // 7. Return the Record { [[Read]]: read, [[Bytes]]: bytes, [[Error]]: none }.
    Decoded {
        read,
        bytes,
        error: None,
    }
}

/// The string argument of the base64 and hex methods, which is never coerced.
fn string_argument(value: &JsValue) -> JsResult<JsString> {
    value
        .as_string()
        .ok_or_else(|| js_error!(TypeError: "argument must be a string"))
}

impl BuiltinTypedArray {
    /// `Uint8Array.fromBase64 ( string [ , options ] )`
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-uint8array.frombase64
    pub(crate) fn from_base64(
        _: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. If string is not a String, throw a TypeError exception.
        let string = string_argument(args.get_or_undefined(0))?;
        // 2. Let opts be ? GetOptionsObject(options).
        let options = get_options_object(args.get_or_undefined(1))?;
        // 3-5. Let alphabet be ? Get(opts, "alphabet") ...
        let alphabet = get_alphabet(&options, context)?;
        // 6-8. Let lastChunkHandling be ? Get(opts, "lastChunkHandling") ...
        let last_chunk_handling = get_last_chunk_handling(&options, context)?;
        // 9. Let result be FromBase64(string, alphabet, lastChunkHandling).
        let result = from_base64(&string.to_vec(), alphabet, last_chunk_handling, usize::MAX);
        // 10. If result.[[Error]] is not none, then throw result.[[Error]].
        if let Some(error) = result.error {
            return Err(error);
        }
        // 11-14. Return a new Uint8Array of result.[[Bytes]].
        new_uint8_array(&result.bytes, context)
    }

    /// `Uint8Array.fromHex ( string )`
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-uint8array.fromhex
    pub(crate) fn from_hex(
        _: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. If string is not a String, throw a TypeError exception.
        let string = string_argument(args.get_or_undefined(0))?;
        // 2. Let result be FromHex(string).
        let result = from_hex(&string.to_vec(), usize::MAX);
        // 3. If result.[[Error]] is not none, then throw result.[[Error]].
        if let Some(error) = result.error {
            return Err(error);
        }
        // 4-7. Return a new Uint8Array of result.[[Bytes]].
        new_uint8_array(&result.bytes, context)
    }

    /// `Uint8Array.prototype.setFromBase64 ( string [ , options ] )`
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-uint8array.prototype.setfrombase64
    pub(crate) fn set_from_base64(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let into be the this value.
        // 2. Perform ? ValidateUint8Array(into).
        let into = validate_uint8_array(this)?;
        // 3. If string is not a String, throw a TypeError exception.
        let string = string_argument(args.get_or_undefined(0))?;
        // 4. Let opts be ? GetOptionsObject(options).
        let options = get_options_object(args.get_or_undefined(1))?;
        // 5-7. Let alphabet be ? Get(opts, "alphabet") ...
        let alphabet = get_alphabet(&options, context)?;
        // 8-10. Let lastChunkHandling be ? Get(opts, "lastChunkHandling") ...
        let last_chunk_handling = get_last_chunk_handling(&options, context)?;
        // 11-13. Let byteLength be TypedArrayLength(taRecord).
        let byte_length = writable_length(&into)?;
        // 14. Let result be FromBase64(string, alphabet, lastChunkHandling, byteLength).
        let result = from_base64(&string.to_vec(), alphabet, last_chunk_handling, byte_length);
        // 15. Let bytes be result.[[Bytes]].
        // 16. Let written be the length of bytes.
        let written = result.bytes.len();
        // 19. Perform SetUint8ArrayBytes(into, bytes).
        set_uint8_array_bytes(&into, &result.bytes);
        // 20. If result.[[Error]] is not none, then throw result.[[Error]].
        if let Some(error) = result.error {
            return Err(error);
        }
        // 21-24. Return { read: result.[[Read]], written }.
        Ok(read_written(result.read, written, context))
    }

    /// `Uint8Array.prototype.setFromHex ( string )`
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-uint8array.prototype.setfromhex
    pub(crate) fn set_from_hex(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let into be the this value.
        // 2. Perform ? ValidateUint8Array(into).
        let into = validate_uint8_array(this)?;
        // 3. If string is not a String, throw a TypeError exception.
        let string = string_argument(args.get_or_undefined(0))?;
        // 4-6. Let byteLength be TypedArrayLength(taRecord).
        let byte_length = writable_length(&into)?;
        // 7. Let result be FromHex(string, byteLength).
        let result = from_hex(&string.to_vec(), byte_length);
        // 8. Let bytes be result.[[Bytes]].
        // 9. Let written be the length of bytes.
        let written = result.bytes.len();
        // 12. Perform SetUint8ArrayBytes(into, bytes).
        set_uint8_array_bytes(&into, &result.bytes);
        // 13. If result.[[Error]] is not none, then throw result.[[Error]].
        if let Some(error) = result.error {
            return Err(error);
        }
        // 14-17. Return { read: result.[[Read]], written }.
        Ok(read_written(result.read, written, context))
    }

    /// `Uint8Array.prototype.toBase64 ( [ options ] )`
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-uint8array.prototype.tobase64
    pub(crate) fn to_base64(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. Perform ? ValidateUint8Array(O).
        let o = validate_uint8_array(this)?;
        // 3. Let opts be ? GetOptionsObject(options).
        let options = get_options_object(args.get_or_undefined(0))?;
        // 4-6. Let alphabet be ? Get(opts, "alphabet") ...
        let alphabet = get_alphabet(&options, context)?;
        // 7. Let omitPadding be ToBoolean(? Get(opts, "omitPadding")).
        let omit_padding = options
            .get(js_string!("omitPadding"), context)?
            .to_boolean();
        // 8. Let toEncode be ? GetUint8ArrayBytes(O).
        let to_encode = get_uint8_array_bytes(&o)?;
        // 9-10. Encode toEncode with the base64 or base64url alphabet of RFC 4648.
        let table = match alphabet {
            Alphabet::Base64 => BASE64_ALPHABET,
            Alphabet::Base64Url => BASE64URL_ALPHABET,
        };
        let mut out = Vec::with_capacity(to_encode.len().div_ceil(3) * 4);
        for group in to_encode.chunks(3) {
            let n = group
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
            let chars = group.len() + 1;
            for i in 0..4 {
                if i < chars {
                    out.push(table[((n >> (18 - 6 * i)) & 63) as usize]);
                } else if !omit_padding {
                    out.push(b'=');
                }
            }
        }
        // 11. Return CodePointsToString(outAscii).
        Ok(JsString::from(String::from_utf8_lossy(&out).as_ref()).into())
    }

    /// `Uint8Array.prototype.toHex ( )`
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-uint8array.prototype.tohex
    pub(crate) fn to_hex(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. Perform ? ValidateUint8Array(O).
        let o = validate_uint8_array(this)?;
        // 3. Let toEncode be ? GetUint8ArrayBytes(O).
        let to_encode = get_uint8_array_bytes(&o)?;
        // 4. Let out be the empty String.
        // 5. For each byte byte of toEncode, do
        //    a-d. Append the two lowercase hexadecimal digits of byte to out.
        let out: String = to_encode.iter().map(|b| format!("{b:02x}")).collect();
        // 6. Return out.
        Ok(JsString::from(out.as_str()).into())
    }
}
