use std::collections::HashMap;

use crate::support::test_support::{
    self, InMemoryWasmTestBuilder, DEFAULT_BLOCK_TIME, STANDARD_PAYMENT_CONTRACT,
};
use contract_ffi::contract_api::argsparser::ArgsParser;
use contract_ffi::value::U512;
use engine_core::engine_state::MAX_PAYMENT;

const GENESIS_ADDR: [u8; 32] = [7u8; 32];

#[derive(Debug)]
enum GetArgContractError {
    MissingArgument0 = 100,
    MissingArgument1 = 101,
    InvalidArgument0 = 200,
    InvalidArgument1 = 201,
}

/// Calls get_arg contract and returns Ok(()) in case no error, or String which is the error message
/// returned by the engine
fn call_get_arg(args: impl ArgsParser) -> Result<(), String> {
    let result = InMemoryWasmTestBuilder::default()
        .run_genesis(GENESIS_ADDR, HashMap::new())
        .exec_with_args(
            GENESIS_ADDR,
            STANDARD_PAYMENT_CONTRACT,
            (U512::from(MAX_PAYMENT),),
            "get_arg.wasm",
            args,
            DEFAULT_BLOCK_TIME,
            [1u8; 32],
        )
        .commit()
        .finish();

    if !result.builder().is_error() {
        return Ok(());
    }

    let response = result
        .builder()
        .get_exec_response(0)
        .expect("should have a response")
        .to_owned();

    let error_message = {
        let execution_result = test_support::get_success_result(&response);
        test_support::get_error_message(execution_result)
    };

    Err(error_message)
}

#[ignore]
#[test]
fn should_use_passed_argument() {
    call_get_arg((String::from("Hello, world!"), U512::from(42)))
        .expect("Should successfuly call get_arg with 2 valid args");
}

#[ignore]
#[test]
fn should_revert_with_missing_arg() {
    assert_eq!(
        call_get_arg(()).expect_err("should fail"),
        format!(
            "Exit code: {}",
            GetArgContractError::MissingArgument0 as u32,
        )
    );
    assert_eq!(
        call_get_arg((String::from("Hello, world!"),)).expect_err("should fail"),
        format!(
            "Exit code: {}",
            GetArgContractError::MissingArgument1 as u32
        )
    );
}

#[ignore]
#[test]
fn should_revert_with_invalid_argument() {
    assert_eq!(
        call_get_arg((U512::from(123),)).expect_err("should fail"),
        format!(
            "Exit code: {}",
            GetArgContractError::InvalidArgument0 as u32
        )
    );
    assert_eq!(
        call_get_arg((
            String::from("Hello, world!"),
            String::from("this is expected to be U512")
        ))
        .expect_err("should fail"),
        format!(
            "Exit code: {}",
            GetArgContractError::InvalidArgument1 as u32
        )
    );
}
