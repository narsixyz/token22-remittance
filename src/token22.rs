use spl_token_2022::{
    extension::{
        default_account_state::instruction::initialize_default_account_state,
        metadata_pointer::instruction::initialize as initialize_metadata_pointer,
        transfer_fee::instruction::initialize_transfer_fee_config,
        ExtensionType,
    },
    instruction::initialize_mint_close_authority,
    state::{AccountState, Mint},
};

use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

pub fn v1_extensions() -> Vec<ExtensionType> {
    vec![
        ExtensionType::TransferFeeConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::DefaultAccountState,
        ExtensionType::MintCloseAuthority,
    ]
}

pub fn v1_mint_space() -> usize {
    ExtensionType::try_calculate_account_len::<Mint>(&v1_extensions())
        .expect("failed to calculate V1 mint size")
}

pub fn v1_initialization_instructions(
    token_program: &Pubkey,
    mint: &Pubkey,
    authority: &Pubkey,
) -> Vec<Instruction> {
    vec![
        initialize_transfer_fee_config(
            token_program,
            mint,
            Some(authority),
            Some(authority),
            100,
            1_000_000,
        )
        .expect("transfer fee initialization failed"),

        initialize_metadata_pointer(
            token_program,
            mint,
            Some(*authority),
            Some(*mint),
        )
        .expect("metadata pointer initialization failed"),

        initialize_default_account_state(
            token_program,
            mint,
            &AccountState::Frozen,
        )
        .expect("default account state initialization failed"),

        initialize_mint_close_authority(
            token_program,
            mint,
            Some(authority),
        )
        .expect("mint close authority initialization failed"),
    ]
}

pub fn read_transfer_fee_config(
    mint_data: &[u8],
) -> Result<spl_token_2022::extension::transfer_fee::TransferFeeConfig, solana_program_error::ProgramError> {
    use spl_token_2022::extension::{
        transfer_fee::TransferFeeConfig,
        BaseStateWithExtensions,
        StateWithExtensions,
    };

    let mint = StateWithExtensions::<Mint>::unpack(mint_data)?;
    Ok(*mint.get_extension::<TransferFeeConfig>()?)
}

pub fn transfer_with_fee(
    token_program: &Pubkey,
    source: &Pubkey,
    mint: &Pubkey,
    destination: &Pubkey,
    authority: &Pubkey,
    amount: u64,
    decimals: u8,
    fee: u64,
) -> Instruction {
    spl_token_2022::extension::transfer_fee::instruction::transfer_checked_with_fee(
        token_program,
        source,
        mint,
        destination,
        authority,
        &[],
        amount,
        decimals,
        fee,
    )
    .expect("failed to build transfer_checked_with_fee instruction")
}

pub fn calculate_current_fee(
    mint_data: &[u8],
    current_epoch: u64,
    amount: u64,
) -> Result<u64, solana_program_error::ProgramError> {
    let transfer_fee = read_transfer_fee_config(mint_data)?;

    transfer_fee
        .calculate_epoch_fee(current_epoch, amount)
        .ok_or(solana_program_error::ProgramError::InvalidInstructionData)
}
