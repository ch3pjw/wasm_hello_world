use {
    enum_primitive::*,
    nom::{
        IResult,
        Parser,
        bits,
        bits::streaming::take as take_bits,
        bytes::streaming::take as take_bytes,
        combinator::{cond, eof, map, map_opt, success},
        number::streaming::{be_u16, be_u32, be_u64},
        sequence::tuple,
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
    pub fn parse(input: &'a [u8]) -> IResult<&[u8], Frame<'a>> {
        frame_p().parse(input)
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


fn frame_p<'a>() -> impl ByteParser<'a, Frame<'a>> {
    bits(
        tuple((
            flag_p(),
            count_unit(3, flag_p().map(|res_flag| assert!(!res_flag))),
            op_code_p(),
            flag_p(),
            take_bits(7usize)
        ))
    ).flat_map(|(fin, _, op, masked, payload_len)| {
        // Annoyingly we need to take the input here as without it, even though the match arms
        // give the same function signatures, they are not of an identical function type:
        (move |inp| match payload_len {
            126 => be_u16.map(|l: u16| l as usize).parse(inp),
            127 => be_u64.map(|l: u64| l as usize).parse(inp),
            _ => success(payload_len as usize).parse(inp)
        }).flat_map(move |payload_len| {
            tuple((
                cond(masked, be_u32),
                take_bytes(payload_len),
                eof
            ))
        }).map(move |(masking_key, raw_payload, _)| {
            Frame { fin, op, masking_key, raw_payload }
        })
    })
}

fn flag_p<'a>() -> impl BitParser<'a, bool> {
    map(take_bits(1usize), |bit: u8| if bit == 0 { false } else { true })
}

enum_from_primitive! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Op{
        Continuation = 0x0,
        Text = 0x1,
        Binary = 0x2,
        Close = 0x8,
        Ping = 0x9,
        Pong = 0xa,
    }
}

fn op_code_p<'a>() -> impl BitParser<'a, Op> {
    map_opt(take_bits(4usize), Op::from_u8)
}

fn count_unit<I, O, E, P: Parser<I, O, E>>(n: usize, mut parser: P) -> impl Parser<I, (), E> {
    move |mut input| {
        for _ in 0..n {
            let (remaining, _) = parser.parse(input)?;
            input = remaining
        }
        Ok((input, ()))
    }
}

type BitStream<'a> = (&'a [u8], usize);

// Define something like "trait aliases":
trait BitParser<'a, O>: Parser<BitStream<'a>, O, nom::error::Error<BitStream<'a>>> {}
impl<'a, T: Parser<BitStream<'a>, O, nom::error::Error<BitStream<'a>>>, O> BitParser<'a, O> for T {}

trait ByteParser<'a, O>: Parser<&'a [u8], O, nom::error::Error<&'a [u8]>> {}
impl<'a, T: Parser<&'a [u8], O, nom::error::Error<&'a [u8]>>, O> ByteParser<'a, O> for T {}


#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    fn op_code_byte_p<'a>() -> impl ByteParser<'a, Op> {
        bits(|inp| op_code_p().parse(inp))
    }

    #[test]
    fn test_parse_op_code() {
        let (_, op_code) = op_code_byte_p().parse(&[0b1001_1111, 1u8]).expect("expected success");
        assert_eq!(op_code, Op::Ping);
    }

    #[test]
    fn test_parse_bad_op_code() {
        op_code_byte_p().parse(&[0b1111_0000, 1u8]).expect_err("expected failure");
    }

    #[test]
    fn test_parse_unmasked() {
        let (_, f) = frame_p().parse(
            &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]
        ).expect("expected success");
        assert!(f.fin);
        assert_eq!(f.op, Op::Text);
        assert_eq!(f.masking_key, None);
        assert_eq!(str::from_utf8(f.raw_payload).expect("not utf8"), "Hello");
    }

    #[test]
    fn test_parse_masked() {
        let (_, f) = frame_p().parse(
            &[0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58]
        ).expect("expected success");
        assert!(f.fin);
        assert_eq!(f.op, Op::Text);
        assert_eq!(f.masking_key, Some(0x37fa213d));
        assert_eq!(f.raw_payload, &[0x7f, 0x9f, 0x4d, 0x51, 0x58]);
        assert_eq!(str::from_utf8(&f.payload()).expect("not utf8"), "Hello");
    }
}
