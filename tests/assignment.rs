use solana_program_test::ProgramTest;
use solana_clock::Clock;
use solana_signer::Signer;
use solana_keypair::Keypair;
use solana_system_interface::instruction as system_instruction;
use spl_token_2022::instruction::initialize_mint;
use token22_remittance::token22::{
    v1_initialization_instructions,
    v1_mint_space,
};

#[tokio::test]
async fn create_v1_mint_fixture() {
    let test = ProgramTest::default();
    let (banks_client, payer, recent_blockhash) = test.start().await;

    let mint = Keypair::new();
    let mint_authority = Keypair::new();
    let freeze_authority = Keypair::new();
    let close_authority = Keypair::new();

    let token_program = spl_token_2022::id();
    let mint_space = v1_mint_space();

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

    let transaction = solana_transaction::Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[&payer, &mint],
        recent_blockhash,
    );

    banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    let mint_account = banks_client.get_account(mint.pubkey()).await.unwrap().unwrap();
    let transfer_fee = token22_remittance::token22::read_transfer_fee_config(&mint_account.data).unwrap();
    assert_eq!(transfer_fee.older_transfer_fee.transfer_fee_basis_points, 100.into());
    assert_eq!(transfer_fee.older_transfer_fee.maximum_fee, 1_000_000.into());
    assert_eq!(transfer_fee.newer_transfer_fee.transfer_fee_basis_points, 100.into());
    assert_eq!(transfer_fee.newer_transfer_fee.maximum_fee, 1_000_000.into());
    let current_fee = token22_remittance::token22::calculate_current_fee(
        &mint_account.data,
        0,
        1_000_000,
    ).unwrap();
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
