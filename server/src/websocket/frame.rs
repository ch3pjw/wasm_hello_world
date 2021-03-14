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
        parse_frame_old(input)
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


fn parse_frame_old(input: &[u8]) -> IResult<&[u8], Frame> {
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


use binread::{BinRead, derive_binread, BinResult, ReadOptions};
use modular_bitfield::prelude::{bitfield, BitfieldSpecifier, B7};
use std::io::Cursor;

#[derive(BitfieldSpecifier, Debug, PartialEq)]
#[bits = 4]
enum Op2 {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xa,
}

#[bitfield]
#[derive(BinRead)]
#[br(map = Self::from_bytes)]
struct RawFrameHeaderPrelude {
    fin: bool,
    reserved_1: bool,
    reserved_2: bool,
    reserved_3: bool,
    op: Op2,
    masked: bool,
    // FIXME: This does not take endianness into account :-/
    payload_len: B7
}

#[derive_binread]
#[br(big)]
struct Frame2 {
    #[br(temp)]
    _hdr: RawFrameHeaderPrelude,
    #[br(calc = _hdr.fin())]
    fin: bool,
    #[br(calc = _hdr.op())]
    op: Op2,
    #[br(temp, parse_with = parse_len, args(_hdr.payload_len()))]
    payload_len: u64,
    #[br(if(_hdr.masked()))]
    masking_key: Option<u32>,
    #[br(count = payload_len)]
    raw_payload: Vec<u8>,
}

impl Frame2 {
    pub fn parse(bytes: &[u8]) -> BinResult<Frame2> {
        let mut cursor = Cursor::new(bytes);
        Frame2::read(&mut cursor)
    }

    pub fn payload(&self) -> Vec<u8> {
        match self.masking_key {
            // FIXME: BAD - copying the whole payload!
            None => self.raw_payload.clone(),
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

fn parse_len<R: binread::io::Read + binread::io::Seek>(
    reader: &mut R, ro: &ReadOptions, args: (u8,)) -> BinResult<u64>
{
    let (payload_len,) = args;
    Ok(match payload_len {
        126 => {
            let mut buf = [0u8; size_of::<u16>()];
            reader.read(&mut buf)?;
            u16::from_be_bytes(buf) as u64
        }
        127 => {
            let mut buf = [0u8; size_of::<u64>()];
            reader.read(&mut buf)?;
            u64::from_be_bytes(buf)
        },
        _ => payload_len as u64
    })
}


// #[derive_binread]
// #[derive(Debug, PartialEq)]
// #[br(import(ty: u8))]
// enum Op2 {
    // #[br(pre_assert(ty == 0x0))] Continuation,
    // #[br(pre_assert(ty == 0x1))] Text,
    // #[br(pre_assert(ty == 0x2))] Binady,
    // #[br(pre_assert(ty == 0x8))] Close,
    // #[br(pre_assert(ty == 0x9))] Ping,
    // #[br(pre_assert(ty == 0xa))] Pong,
// }


// #[derive_binread]
// pub struct RawFrame<'a> {
    // #[br(temp)]                                  _hdr_byte_1: u8,
    // #[br(calc = _hdr_byte_1 & 1 > 0)]            fin: bool,
    // #[br(calc = _hdr_byte_1 & 2 > 0)]            reserved_1: bool,
    // #[br(calc = _hdr_byte_1 & 4 > 0)]            reserved_2: bool,
    // #[br(calc = _hdr_byte_1 & 8 > 0)]            reserved_2: bool,
    // #[br(args((_hdr_byte_1 & 0b11110000) >> 4))] op: Op,

    // #[br(temp)]                                  _hdr_byte_2: u8
    // #[br(calc = _hdr_byte_2 & 1 > 0, temp)]      _is_masked: bool
    // #[br(calc = (_hdr_byte_2 & 0b1111110) >> 1)] 

    // masking_key: Option<u32>,
    // raw_payload: &'a [u8],
// }


#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    fn parse_op_code_bytes(input: &[u8]) -> IResult<&[u8], Op> {
        bits(parse_op_code)(input)
    }

    // #[test]
    // fn test_parse_op2_code() {
        // let mut reader = Cursor::new(&[0b1001_1111, 1u8]);
        // let op_code = Op2::read(&mut reader).unwrap();
        // assert_eq!(op_code, Op2::Ping);
    // }

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
        let f = Frame2::parse(
            &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]
        ).expect("expected success");
        assert!(f.fin);
        assert_eq!(f.op, Op2::Text);
        assert_eq!(f.masking_key, None);
        assert_eq!(str::from_utf8(&f.raw_payload).expect("not utf8"), "Hello");
    }

    #[test]
    fn test_parse_masked() {
        let f = Frame2::parse(
            &[0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58]
        ).expect("expected success");
        assert!(f.fin);
        assert_eq!(f.op, Op2::Text);
        assert_eq!(f.masking_key, Some(0x37fa213d));
        assert_eq!(f.raw_payload, vec![0x7f, 0x9f, 0x4d, 0x51, 0x58]);
        assert_eq!(str::from_utf8(&f.payload()).expect("not utf8"), "Hello");
    }
}
