use std::num::ParseIntError;
use std::{fmt, u64};

use mp_starknet::execution::types::Felt252Wrapper;
use starknet_ff::FieldElement;

pub struct RpcEventFilter {
    pub from_block: u64,
    pub to_block: u64,
    pub from_address: Option<Felt252Wrapper>,
    pub keys: Vec<Vec<FieldElement>>,
    pub chunk_size: u64,
    pub continuation_token: ContinuationToken,
}

#[derive(PartialEq, Eq, Debug, Default)]
pub struct ContinuationToken {
    pub block_n: u64,
    pub event_n: u64,
}

#[derive(PartialEq, Eq, Debug)]
pub enum ParseTokenError {
    WrongToken,
    ParseFailed(ParseIntError),
}

impl fmt::Display for ContinuationToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:x},{:x}", self.block_n, self.event_n)
    }
}

impl ContinuationToken {
    pub fn parse(token: String) -> Result<Self, ParseTokenError> {
        let arr: Vec<&str> = token.split(',').collect();
        if arr.len() != 2 {
            return Err(ParseTokenError::WrongToken);
        }
        let block_n = u64::from_str_radix(arr[0], 16).map_err(ParseTokenError::ParseFailed)?;
        let event_n = u64::from_str_radix(arr[1], 16).map_err(ParseTokenError::ParseFailed)?;

        Ok(ContinuationToken { block_n, event_n })
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::types::*;

    #[rstest]
    #[case(0, 0, "0,0")]
    #[case(1, 4, "1,4")]
    #[case(2, 4, "2,4")]
    #[case(30, 4, "1e,4")]
    #[case(0, 4, "0,4")]
    fn to_string_works(#[case] block_n: u64, #[case] event_n: u64, #[case] expected: String) {
        let token = ContinuationToken { block_n, event_n };
        assert_eq!(expected, token.to_string())
    }

    #[rstest]
    #[case("0,0", 0, 0)]
    #[case("1,4", 1, 4)]
    #[case("2,4", 2, 4)]
    #[case("1e,4", 30, 4)]
    #[case("244,1", 2*16*16+4*16+4, 1)]
    fn parse_works(#[case] string_token: String, #[case] block_n: u64, #[case] event_n: u64) {
        let expected = ContinuationToken { block_n, event_n };
        assert_eq!(expected, ContinuationToken::parse(string_token).unwrap());
    }

    #[rstest]
    #[case("100")]
    #[case("0,")]
    #[case("0,0,0")]
    fn parse_should_fail(#[case] string_token: String) {
        let result = ContinuationToken::parse(string_token);
        assert!(result.is_err());
    }

    #[rstest]
    #[case("2y,4")]
    #[case("30,255g")]
    #[case("1,1,")]
    fn parse_u64_should_fail(#[case] string_token: String) {
        let result = ContinuationToken::parse(string_token);
        assert!(result.is_err());
    }
}
