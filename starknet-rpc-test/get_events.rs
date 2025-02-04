extern crate starknet_rpc_test;

use assert_matches::assert_matches;
use rstest::rstest;
use starknet_accounts::Account;
use starknet_core::types::{BlockId, EmittedEvent, EventFilter, StarknetError};
use starknet_core::utils::get_selector_from_name;
use starknet_ff::FieldElement;
use starknet_providers::jsonrpc::HttpTransport;
use starknet_providers::{JsonRpcClient, MaybeUnknownErrorCode, Provider, ProviderError, StarknetErrorWithMessage};
use starknet_rpc_test::constants::{ARGENT_CONTRACT_ADDRESS, FEE_TOKEN_ADDRESS, SEQUENCER_ADDRESS, SIGNER_PRIVATE};
use starknet_rpc_test::fixtures::madara;
use starknet_rpc_test::utils::{assert_eq_emitted_event, create_account, AccountActions};
use starknet_rpc_test::{MadaraClient, Transaction, TransactionResult};

async fn transfer_tokens(
    rpc: &JsonRpcClient<HttpTransport>,
    madara: &MadaraClient,
    recipient: FieldElement,
    transfer_amount: FieldElement,
) -> (FieldElement, FieldElement) {
    let account = create_account(rpc, SIGNER_PRIVATE, ARGENT_CONTRACT_ADDRESS, true);
    let mut txs = madara
        .create_block_with_txs(vec![Transaction::Execution(account.transfer_tokens(recipient, transfer_amount, None))])
        .await
        .unwrap();
    assert_eq!(txs.len(), 1);
    let transaction_hash = match txs.remove(0).unwrap() {
        TransactionResult::Execution(response) => response.transaction_hash,
        _ => panic!("Expected execution response"),
    };
    (transaction_hash, account.address())
}

#[rstest]
#[tokio::test]
async fn fail_invalid_continuation_token(#[future] madara: MadaraClient) -> Result<(), anyhow::Error> {
    let madara = madara.await;
    let rpc = madara.get_starknet_client();

    let events_result = rpc
        .get_events(
            EventFilter {
                from_block: Some(BlockId::Number(0)),
                to_block: Some(BlockId::Number(5)),
                address: None,
                keys: None,
            },
            Some("0,100,0".into()),
            100,
        )
        .await;

    assert_matches!(
        events_result,
        Err(ProviderError::StarknetError(StarknetErrorWithMessage {
            message: _,
            code: MaybeUnknownErrorCode::Known(StarknetError::InvalidContinuationToken)
        }))
    );

    Ok(())
}

#[rstest]
#[tokio::test]
async fn fail_chunk_size_too_big(#[future] madara: MadaraClient) -> Result<(), anyhow::Error> {
    let madara = madara.await;
    let rpc = madara.get_starknet_client();

    let events_result = rpc
        .get_events(
            EventFilter {
                from_block: Some(BlockId::Number(0)),
                to_block: Some(BlockId::Number(5)),
                address: None,
                keys: None,
            },
            None,
            1001,
        )
        .await;

    assert_matches!(
        events_result,
        Err(ProviderError::StarknetError(StarknetErrorWithMessage {
            message: _,
            code: MaybeUnknownErrorCode::Known(StarknetError::PageSizeTooBig)
        }))
    );

    Ok(())
}

#[rstest]
#[tokio::test]
async fn fail_keys_too_big(#[future] madara: MadaraClient) -> Result<(), anyhow::Error> {
    let madara = madara.await;
    let rpc = madara.get_starknet_client();

    let events_result = rpc
        .get_events(
            EventFilter {
                from_block: Some(BlockId::Number(0)),
                to_block: Some(BlockId::Number(5)),
                address: None,
                keys: Some(vec![vec![FieldElement::ZERO]; 101]),
            },
            None,
            10,
        )
        .await;

    assert_matches!(
        events_result,
        Err(ProviderError::StarknetError(StarknetErrorWithMessage {
            message: _,
            code: MaybeUnknownErrorCode::Known(StarknetError::TooManyKeysInFilter)
        }))
    );

    Ok(())
}

#[rstest]
#[tokio::test]
async fn work_one_block_no_filter(#[future] madara: MadaraClient) -> Result<(), anyhow::Error> {
    let madara = madara.await;
    let rpc = madara.get_starknet_client();

    let recipient = FieldElement::from_hex_be("0x123").unwrap();
    let transfer_amount = FieldElement::ONE;
    let (transaction_hash, account_address) = transfer_tokens(rpc, &madara, recipient, transfer_amount).await;

    let events_result = rpc
        .get_events(EventFilter { from_block: None, to_block: None, address: None, keys: None }, None, 10)
        .await
        .unwrap();

    let fee_token_address = FieldElement::from_hex_be(FEE_TOKEN_ADDRESS).unwrap();
    let block_hash =
        FieldElement::from_hex_be("0x0742520489186d3d79b09e1d14ec7e69d515a3c915e6cfd8fd4ca65299372a45").unwrap();
    let block_number = 1;
    let expected_fee = FieldElement::from_hex_be("0x1d010").unwrap();

    assert_eq_emitted_event(
        events_result.events,
        vec![
            EmittedEvent {
                from_address: fee_token_address,
                keys: vec![get_selector_from_name("Transfer").unwrap()],
                data: vec![
                    account_address,    // from
                    recipient,          // to
                    transfer_amount,    // value low
                    FieldElement::ZERO, // value high
                ],
                block_hash,
                block_number,
                transaction_hash,
            },
            EmittedEvent {
                from_address: account_address,
                keys: vec![get_selector_from_name("transaction_executed").unwrap()],
                data: vec![
                    transaction_hash,  // txn hash
                    FieldElement::TWO, // response_len
                    FieldElement::ONE,
                    FieldElement::ONE,
                ],
                block_hash,
                block_number,
                transaction_hash,
            },
            EmittedEvent {
                from_address: fee_token_address,
                keys: vec![get_selector_from_name("Transfer").unwrap()],
                data: vec![
                    account_address,                                       // from
                    FieldElement::from_hex_be(SEQUENCER_ADDRESS).unwrap(), // to (sequencer address)
                    expected_fee,                                          // value low
                    FieldElement::ZERO,                                    // value high
                ],
                block_hash,
                block_number,
                transaction_hash,
            },
        ],
    );
    assert_eq!(events_result.continuation_token, None);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn work_one_block_with_chunk_filter_and_continuation_token(
    #[future] madara: MadaraClient,
) -> Result<(), anyhow::Error> {
    let madara = madara.await;
    let rpc = madara.get_starknet_client();

    let recipient = FieldElement::from_hex_be("0x123").unwrap();
    let transfer_amount = FieldElement::ONE;
    let (transaction_hash, account_address) = transfer_tokens(rpc, &madara, recipient, transfer_amount).await;

    let events_result = rpc
        .get_events(EventFilter { from_block: None, to_block: None, address: None, keys: None }, None, 1)
        .await
        .unwrap();

    let fee_token_address = FieldElement::from_hex_be(FEE_TOKEN_ADDRESS).unwrap();
    let block_hash =
        FieldElement::from_hex_be("0x0742520489186d3d79b09e1d14ec7e69d515a3c915e6cfd8fd4ca65299372a45").unwrap();
    let block_number = 1;

    assert_eq_emitted_event(
        events_result.events,
        vec![EmittedEvent {
            from_address: fee_token_address,
            keys: vec![get_selector_from_name("Transfer").unwrap()],
            data: vec![
                account_address,    // from
                recipient,          // to
                transfer_amount,    // value low
                FieldElement::ZERO, // value high
            ],
            block_hash,
            block_number,
            transaction_hash,
        }],
    );
    assert_eq!(events_result.continuation_token, Some("1,1".into()));

    let events_result = rpc
        .get_events(
            EventFilter { from_block: None, to_block: None, address: None, keys: None },
            events_result.continuation_token,
            10,
        )
        .await
        .unwrap();

    let expected_fee = FieldElement::from_hex_be("0x1d010").unwrap();
    assert_eq_emitted_event(
        events_result.events,
        vec![
            EmittedEvent {
                from_address: account_address,
                keys: vec![get_selector_from_name("transaction_executed").unwrap()],
                data: vec![
                    transaction_hash,  // txn hash
                    FieldElement::TWO, // response_len
                    FieldElement::ONE,
                    FieldElement::ONE,
                ],
                block_hash,
                block_number,
                transaction_hash,
            },
            EmittedEvent {
                from_address: fee_token_address,
                keys: vec![get_selector_from_name("Transfer").unwrap()],
                data: vec![
                    account_address,                                       // from
                    FieldElement::from_hex_be(SEQUENCER_ADDRESS).unwrap(), // to (sequencer address)
                    expected_fee,                                          // value low
                    FieldElement::ZERO,                                    // value high
                ],
                block_hash,
                block_number,
                transaction_hash,
            },
        ],
    );

    Ok(())
}

#[rstest]
#[tokio::test]
async fn work_two_blocks_with_block_filter_and_continuation_token(
    #[future] madara: MadaraClient,
) -> Result<(), anyhow::Error> {
    let madara = madara.await;
    let rpc = madara.get_starknet_client();

    let recipient = FieldElement::from_hex_be("0x123").unwrap();
    let transfer_amount = FieldElement::ONE;

    // first block
    let (transaction_hash_1, account_address) = transfer_tokens(rpc, &madara, recipient, transfer_amount).await;
    // second block
    let (transaction_hash_2, _) = transfer_tokens(rpc, &madara, recipient, transfer_amount).await;

    // get first event of first block
    let events_result = rpc
        .get_events(
            EventFilter {
                from_block: Some(BlockId::Number(1)),
                to_block: Some(BlockId::Number(1)),
                address: None,
                keys: None,
            },
            None,
            1,
        )
        .await
        .unwrap();

    let fee_token_address = FieldElement::from_hex_be(FEE_TOKEN_ADDRESS).unwrap();

    assert_eq_emitted_event(
        events_result.events,
        vec![EmittedEvent {
            from_address: fee_token_address,
            keys: vec![get_selector_from_name("Transfer").unwrap()],
            data: vec![
                account_address,    // from
                recipient,          // to
                transfer_amount,    // value low
                FieldElement::ZERO, // value high
            ],
            block_hash: FieldElement::from_hex_be("0x0742520489186d3d79b09e1d14ec7e69d515a3c915e6cfd8fd4ca65299372a45")
                .unwrap(),
            block_number: 1,
            transaction_hash: transaction_hash_1,
        }],
    );
    assert_eq!(events_result.continuation_token, Some("0,1".into()));

    // get first event of second block
    let events_result = rpc
        .get_events(
            EventFilter {
                from_block: Some(BlockId::Number(2)),
                to_block: Some(BlockId::Number(2)),
                address: None,
                keys: None,
            },
            None,
            1,
        )
        .await
        .unwrap();

    assert_eq_emitted_event(
        events_result.events,
        vec![EmittedEvent {
            from_address: fee_token_address,
            keys: vec![get_selector_from_name("Transfer").unwrap()],
            data: vec![
                account_address,    // from
                recipient,          // to
                transfer_amount,    // value low
                FieldElement::ZERO, // value high
            ],
            block_hash: FieldElement::from_hex_be("0x04335858071beb1eeba6b5cea1c9559297a8bef1132ae6a84a41d6c6a951ac3f")
                .unwrap(),
            block_number: 2,
            transaction_hash: transaction_hash_2,
        }],
    );

    assert_eq!(events_result.continuation_token, Some("0,1".into()));

    Ok(())
}

#[rstest]
#[tokio::test]
async fn work_one_block_address_filter(#[future] madara: MadaraClient) -> Result<(), anyhow::Error> {
    let madara = madara.await;
    let rpc = madara.get_starknet_client();

    let recipient = FieldElement::from_hex_be("0x123").unwrap();
    let transfer_amount = FieldElement::ONE;
    let (transaction_hash, account_address) = transfer_tokens(rpc, &madara, recipient, transfer_amount).await;

    let events_result = rpc
        .get_events(
            EventFilter { from_block: None, to_block: None, address: Some(account_address), keys: None },
            None,
            10,
        )
        .await
        .unwrap();

    let block_hash =
        FieldElement::from_hex_be("0x0742520489186d3d79b09e1d14ec7e69d515a3c915e6cfd8fd4ca65299372a45").unwrap();
    let block_number = 1;

    assert_eq_emitted_event(
        events_result.events,
        vec![EmittedEvent {
            from_address: account_address,
            keys: vec![get_selector_from_name("transaction_executed").unwrap()],
            data: vec![
                transaction_hash,  // txn hash
                FieldElement::TWO, // response_len
                FieldElement::ONE,
                FieldElement::ONE,
            ],
            block_hash,
            block_number,
            transaction_hash,
        }],
    );
    assert_eq!(events_result.continuation_token, None);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn work_one_block_key_filter(#[future] madara: MadaraClient) -> Result<(), anyhow::Error> {
    let madara = madara.await;
    let rpc = madara.get_starknet_client();

    let recipient = FieldElement::from_hex_be("0x123").unwrap();
    let transfer_amount = FieldElement::ONE;
    let (transaction_hash, account_address) = transfer_tokens(rpc, &madara, recipient, transfer_amount).await;
    let key = get_selector_from_name("transaction_executed").unwrap();

    let events_result = rpc
        .get_events(
            EventFilter { from_block: None, to_block: None, address: None, keys: Some(vec![vec![key]]) },
            None,
            10,
        )
        .await
        .unwrap();

    let block_hash =
        FieldElement::from_hex_be("0x0742520489186d3d79b09e1d14ec7e69d515a3c915e6cfd8fd4ca65299372a45").unwrap();
    let block_number = 1;

    assert_eq_emitted_event(
        events_result.events,
        vec![EmittedEvent {
            from_address: account_address,
            keys: vec![key],
            data: vec![
                transaction_hash,  // txn hash
                FieldElement::TWO, // response_len
                FieldElement::ONE,
                FieldElement::ONE,
            ],
            block_hash,
            block_number,
            transaction_hash,
        }],
    );
    assert_eq!(events_result.continuation_token, None);

    Ok(())
}
