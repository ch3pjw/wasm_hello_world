use {
    enum_primitive::*,
    nom::{
        IResult,
        bits,
        bits::streaming::take as take_bits,
        bytes::streaming::take as take_bytes,
        combinator::{eof, map, map_opt},
        number::streaming::{be_u16, be_u32, be_u64},
    },
    std::{
        convert::TryInto,
        mem::size_of,
    },
};


pub struct Frame<'a> {
    fin: bool,
    op: Op,
    masking_key: Option<u32>,
    raw_payload: &'a [u8],
}

impl<'a> Frame<'a> {
    // Currently trivial, but might want different error handling:
    pub fn parse(input: &[u8]) -> IResult<&[u8], Frame> {
        parse_frame(input)
    }

    pub fn payload(&self) -> Vec<u8> {
        match self.masking_key {
            None => self.raw_payload.to_vec(),
            Some(key) => {
                let mut v = Vec::with_capacity(self.raw_payload.len());
                // I'm attempting to be efficient about this by doing the xor in 4-byte chunks:
                let mut iter = self.raw_payload.chunks_exact(size_of::<u32>());
                for bytes in &mut iter {
                    let u = u32::from_be_bytes(bytes.try_into().expect("size mismatch"));
                    v.extend(&(u ^ key).to_be_bytes());
                }
                for (data, key) in iter.remainder().iter().zip(&key.to_be_bytes()) {
                    v.push(data ^ key);
                };
                v
            },
        }
    }
}


fn parse_frame(input: &[u8]) -> IResult<&[u8], Frame> {
    let (input, (fin, op, masked, payload_len)) = bits(parse_frame_head)(input)?;
    let (input, payload_len) = match payload_len {
        126 => {
            let (input, payload_len) = be_u16(input)?;
            (input, payload_len as usize)
        },
        127 => {
            let (input, payload_len) = be_u64(input)?;
            (input, payload_len as usize)
        },
        _ => (input, payload_len as usize)
    };
    let (input, masking_key) = if masked {
        map(be_u32, |n| Some(n))(input)?
    } else {
        (input, None)
    };
    let (input, raw_payload) = take_bytes(payload_len)(input)?;
    eof(input)?;
    Ok((input, Frame {fin, op, masking_key, raw_payload}))
}

fn parse_frame_head(input: BitStream) -> IResult<BitStream, (bool, Op, bool, u8)> {
    let (mut input, fin) = parse_flag(input)?;
    for _ in 0..3 {
        let (i, reserved_flag) = parse_flag(input)?;
        input = i;
        assert!(!reserved_flag)
    }
    let (input, op) = parse_op_code(input)?;
    let (input, masked) = parse_flag(input)?;
    let (input, payload_len) = parse_payload_len(input)?;
    Ok((input, (fin, op, masked, payload_len)))
}


fn parse_flag(input: BitStream) -> IResult<BitStream, bool> {
    map(take_bits(1usize), |bit: u8| if bit == 0 { false } else { true })(input)
}

enum_from_primitive! {
    #[derive(Debug, PartialEq)]
    enum Op{
        Continuation = 0x0,
        Text = 0x1,
        Binary = 0x2,
        Close = 0x8,
        Ping = 0x9,
        Pong = 0xa,
    }
}

fn parse_op_code(input: BitStream) -> IResult<BitStream, Op> {
    map_opt(take_bits(4usize), Op::from_u8)(input)
}

fn parse_payload_len(input: BitStream) -> IResult<BitStream, u8> {
    take_bits(7usize)(input)
}

type BitStream<'a> = (&'a [u8], usize);
type NomError<'a> = (&'a [u8], nom::error::ErrorKind);


#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    fn parse_op_code_bytes(input: &[u8]) -> IResult<&[u8], Op> {
        bits(parse_op_code)(input)
    }

    #[test]
    fn test_parse_op_code() {
        let (_, op_code) = parse_op_code_bytes(&[0b1001_1111, 1u8]).expect("expected success");
        assert_eq!(op_code, Op::Ping);
    }

    #[test]
    fn test_parse_bad_op_code() {
        parse_op_code_bytes(&[0b1111_0000, 1u8]).expect_err("expected failure");
    }

    #[test]
    fn test_parse_unmasked() {
        let (_, f) = parse_frame(
            &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]
        ).expect("expected success");
        assert!(f.fin);
        assert_eq!(f.op, Op::Text);
        assert_eq!(f.masking_key, None);
        assert_eq!(str::from_utf8(f.raw_payload).expect("not utf8"), "Hello");
    }

    #[test]
    fn test_parse_masked() {
        let (_, f) = parse_frame(
            &[0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58]
        ).expect("expected success");
        assert!(f.fin);
        assert_eq!(f.op, Op::Text);
        assert_eq!(f.masking_key, Some(0x37fa213d));
        assert_eq!(f.raw_payload, &[0x7f, 0x9f, 0x4d, 0x51, 0x58]);
        assert_eq!(str::from_utf8(&f.payload()).expect("not utf8"), "Hello");
    }
}
