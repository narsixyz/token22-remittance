use solana_program_test::ProgramTest;
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
    let (mut banks_client, payer, recent_blockhash) = test.start().await;

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

    println!("V1 mint created successfully");
    println!("mint: {}", mint.pubkey());
    println!("space: {} bytes", mint_space);
}
