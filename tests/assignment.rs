use solana_program_test::ProgramTest;
use token22_remittance::token22::{v1_extensions, v1_mint_space};
use spl_token_2022::extension::ExtensionType;

#[tokio::test]
async fn create_v1_mint_fixture() {
    let _test = ProgramTest::default();

    let extensions = v1_extensions();

    assert!(extensions.contains(&ExtensionType::TransferFeeConfig));
    assert!(extensions.contains(&ExtensionType::MetadataPointer));
    assert!(extensions.contains(&ExtensionType::DefaultAccountState));
    assert!(extensions.contains(&ExtensionType::MintCloseAuthority));

    let space = v1_mint_space();

    println!("V1 mint extensions: {:?}", extensions);
    println!("V1 mint space: {} bytes", space);

    assert!(space > 82);
}
