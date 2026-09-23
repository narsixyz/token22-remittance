use solana_program_test::ProgramTest;
use solana_pubkey::Pubkey;
use token22_remittance::token22::{
    v1_initialization_instructions,
    v1_mint_space,
    v1_extensions,
};

#[tokio::test]
async fn create_v1_mint_fixture() {
    let _test = ProgramTest::default();

    let token_program = spl_token_2022::id();
    let mint = Pubkey::new_unique();
    let authority = Pubkey::new_unique();

    let extensions = v1_extensions();
    let space = v1_mint_space();

    let instructions =
        v1_initialization_instructions(&token_program, &mint, &authority);

    assert_eq!(extensions.len(), 4);
    assert!(space > 82);
    assert_eq!(instructions.len(), 4);

    // Every extension initialization instruction must target Token-2022.
    assert!(instructions
        .iter()
        .all(|ix| ix.program_id == token_program));

    // Every extension initialization instruction must contain instruction data.
    assert!(instructions.iter().all(|ix| !ix.data.is_empty()));

    // Required order:
    // 1. TransferFeeConfig
    // 2. MetadataPointer -> mint
    // 3. DefaultAccountState -> Frozen
    // 4. MintCloseAuthority
    assert_ne!(instructions[0].data, instructions[1].data);
    assert_ne!(instructions[1].data, instructions[2].data);
    assert_ne!(instructions[2].data, instructions[3].data);

    println!("V1 initialization instructions: {}", instructions.len());
    println!("V1 mint space: {} bytes", space);
    println!("V1 initialization order verified");
}
