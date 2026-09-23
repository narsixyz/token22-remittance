use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_program_test::ProgramTest;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;
use spl_token_2022::instruction::initialize_mint;
use token22_remittance::token22::calculate_current_fee;
use token22_remittance::token22::initialize_token_account;
use token22_remittance::token22::mint_tokens;
use token22_remittance::token22::read_token_account;
use token22_remittance::token22::read_transfer_fee_amount;
use token22_remittance::token22::thaw_token_account;
use token22_remittance::token22::transfer_with_fee;
use token22_remittance::token22::{v1_initialization_instructions, v1_mint_space};
use token22_remittance::token22::{
    v2_initialization_instructions, v2_mint_space, v2_token_account_space,
};

#[tokio::test]
async fn create_v1_mint_fixture() {
    let test = ProgramTest::default();
    let (banks_client, payer, recent_blockhash) = test.start().await;
    let current_epoch = banks_client.get_sysvar::<Clock>().await.unwrap().epoch;
    let current_epoch = banks_client.get_sysvar::<Clock>().await.unwrap().epoch;

    let mint = Keypair::new();
    let mint_authority = Keypair::new();
    let freeze_authority = Keypair::new();
    let close_authority = Keypair::new();
    let source_account = Keypair::new();
    let destination_account = Keypair::new();
    let source_owner = Keypair::new();
    let destination_owner = Keypair::new();

    let token_program = spl_token_2022::id();
    let mint_space = v1_mint_space();
    let token_account_space = token22_remittance::token22::token_account_space();
    let token_account_rent = banks_client
        .get_rent()
        .await
        .unwrap()
        .minimum_balance(token_account_space);

    let rent = banks_client
        .get_rent()
        .await
        .unwrap()
        .minimum_balance(mint_space);

    let mut instructions = vec![
        system_instruction::create_account(
            &payer.pubkey(),
            &mint.pubkey(),
            rent,
            mint_space as u64,
            &token_program,
        ),
        system_instruction::create_account(
            &payer.pubkey(),
            &source_account.pubkey(),
            token_account_rent,
            token_account_space as u64,
            &token_program,
        ),
        system_instruction::create_account(
            &payer.pubkey(),
            &destination_account.pubkey(),
            token_account_rent,
            token_account_space as u64,
            &token_program,
        ),
    ];

    instructions.extend(v1_initialization_instructions(
        &token_program,
        &mint.pubkey(),
        &close_authority.pubkey(),
    ));

    instructions.push(
        initialize_mint(
            &token_program,
            &mint.pubkey(),
            &mint_authority.pubkey(),
            Some(&freeze_authority.pubkey()),
            6,
        )
        .unwrap(),
    );

    instructions.push(initialize_token_account(
        &token_program,
        &source_account.pubkey(),
        &mint.pubkey(),
        &source_owner.pubkey(),
    ));

    instructions.push(initialize_token_account(
        &token_program,
        &destination_account.pubkey(),
        &mint.pubkey(),
        &destination_owner.pubkey(),
    ));

    // Accounts inherit the mint's DefaultAccountState::Frozen.
    // KYC approval is represented by the freeze authority thawing the account.
    instructions.push(thaw_token_account(
        &token_program,
        &source_account.pubkey(),
        &mint.pubkey(),
        &freeze_authority.pubkey(),
    ));

    instructions.push(thaw_token_account(
        &token_program,
        &destination_account.pubkey(),
        &mint.pubkey(),
        &freeze_authority.pubkey(),
    ));

    instructions.push(mint_tokens(
        &token_program,
        &mint.pubkey(),
        &source_account.pubkey(),
        &mint_authority.pubkey(),
        2_000_000,
    ));

    let thaw_start = instructions.len() - 3;

    let setup_transaction = solana_transaction::Transaction::new_signed_with_payer(
        &instructions[..thaw_start],
        Some(&payer.pubkey()),
        &[&payer, &mint, &source_account, &destination_account],
        recent_blockhash,
    );

    banks_client
        .process_transaction(setup_transaction)
        .await
        .unwrap();

    let source_account_data = banks_client
        .get_account(source_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;

    let destination_account_data = banks_client
        .get_account(destination_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;

    let source_frozen = read_token_account(&source_account_data).unwrap();
    let destination_frozen = read_token_account(&destination_account_data).unwrap();

    assert_eq!(
        source_frozen.state,
        spl_token_2022::state::AccountState::Frozen
    );
    assert_eq!(
        destination_frozen.state,
        spl_token_2022::state::AccountState::Frozen
    );

    let kyc_transaction = solana_transaction::Transaction::new_signed_with_payer(
        &instructions[thaw_start..],
        Some(&payer.pubkey()),
        &[&payer, &freeze_authority, &mint_authority],
        recent_blockhash,
    );

    banks_client
        .process_transaction(kyc_transaction)
        .await
        .unwrap();

    let mint_data = banks_client
        .get_account(mint.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;

    let transfer_amount = 1_000_000;
    let fee = calculate_current_fee(&mint_data, current_epoch, transfer_amount).unwrap();

    assert_eq!(fee, 10_000);

    let transfer_instruction = transfer_with_fee(
        &token_program,
        &source_account.pubkey(),
        &mint.pubkey(),
        &destination_account.pubkey(),
        &source_owner.pubkey(),
        transfer_amount,
        6,
        fee,
    );

    let transfer_transaction = solana_transaction::Transaction::new_signed_with_payer(
        &[transfer_instruction],
        Some(&payer.pubkey()),
        &[&payer, &source_owner],
        recent_blockhash,
    );

    banks_client
        .process_transaction(transfer_transaction)
        .await
        .unwrap();

    let source_account_data = banks_client
        .get_account(source_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;

    let destination_account_data = banks_client
        .get_account(destination_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;

    let source_state = read_token_account(&source_account_data).unwrap();
    let destination_state = read_token_account(&destination_account_data).unwrap();

    assert_eq!(source_state.amount, 1_000_000);
    assert_eq!(destination_state.amount, 990_000);

    let destination_withheld_fee = read_transfer_fee_amount(&destination_account_data).unwrap();

    assert_eq!(destination_withheld_fee, 10_000);

    let mint_account = banks_client
        .get_account(mint.pubkey())
        .await
        .unwrap()
        .unwrap();
    let transfer_fee =
        token22_remittance::token22::read_transfer_fee_config(&mint_account.data).unwrap();
    assert_eq!(
        transfer_fee.older_transfer_fee.transfer_fee_basis_points,
        100.into()
    );
    assert_eq!(
        transfer_fee.older_transfer_fee.maximum_fee,
        1_000_000.into()
    );
    assert_eq!(
        transfer_fee.newer_transfer_fee.transfer_fee_basis_points,
        100.into()
    );
    assert_eq!(
        transfer_fee.newer_transfer_fee.maximum_fee,
        1_000_000.into()
    );
    let current_fee = token22_remittance::token22::calculate_current_fee(
        &mint_account.data,
        current_epoch,
        1_000_000,
    )
    .unwrap();
    assert_eq!(current_fee, 10_000);
    println!("V1 mint created successfully");
    println!("mint: {}", mint.pubkey());
    println!("space: {} bytes", mint_space);
}

#[test]
fn calculate_transfer_fee_from_current_epoch() {
    use spl_token_2022::extension::transfer_fee::TransferFee;

    let fee = TransferFee {
        epoch: 0.into(),
        maximum_fee: 1_000_000.into(),
        transfer_fee_basis_points: 100.into(),
    };

    assert_eq!(fee.calculate_fee(1_000_000), Some(10_000));
    assert_eq!(fee.calculate_fee(100), Some(1));
}

#[test]
fn builds_transfer_checked_with_fee() {
    let token_program = spl_token_2022::id();
    let source = solana_pubkey::Pubkey::new_unique();
    let mint = solana_pubkey::Pubkey::new_unique();
    let destination = solana_pubkey::Pubkey::new_unique();
    let authority = solana_pubkey::Pubkey::new_unique();

    let ix = token22_remittance::token22::transfer_with_fee(
        &token_program,
        &source,
        &mint,
        &destination,
        &authority,
        1_000_000,
        6,
        10_000,
    );

    assert_eq!(ix.program_id, token_program);
    assert_eq!(ix.accounts.len(), 4);
    assert_eq!(ix.accounts[0].pubkey, source);
    assert_eq!(ix.accounts[1].pubkey, mint);
    assert_eq!(ix.accounts[2].pubkey, destination);
    assert_eq!(ix.accounts[3].pubkey, authority);
}

#[tokio::test]
async fn task5_v2_mint_initializes() {
    let program_test = ProgramTest::default();
    let (mut banks_client, payer, recent_blockhash) = program_test.start().await;

    let token_program = spl_token_2022::id();
    let mint = Keypair::new();

    let mint_authority = Keypair::new();
    let freeze_authority = Keypair::new();
    let permanent_delegate = Keypair::new();

    let mint_space = v2_mint_space();

    let rent = banks_client.get_rent().await.expect("failed to get rent");

    let mint_rent = rent.minimum_balance(mint_space);

    let mut instructions = vec![system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        mint_rent,
        mint_space as u64,
        &token_program,
    )];

    instructions.extend(v2_initialization_instructions(
        &token_program,
        &mint.pubkey(),
        &mint_authority.pubkey(),
        &permanent_delegate.pubkey(),
    ));

    instructions.push(
        initialize_mint(
            &token_program,
            &mint.pubkey(),
            &mint_authority.pubkey(),
            Some(&freeze_authority.pubkey()),
            6,
        )
        .expect("failed to build initialize mint"),
    );

    let transaction = solana_transaction::Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[&payer, &mint],
        recent_blockhash,
    );

    banks_client
        .process_transaction(transaction)
        .await
        .expect("V2 mint initialization failed");
}


