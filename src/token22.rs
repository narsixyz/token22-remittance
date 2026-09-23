use spl_token_2022::{
    extension::{
        confidential_transfer::{ConfidentialTransferAccount, ConfidentialTransferMint},
        confidential_transfer_fee::ConfidentialTransferFeeAmount,
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
        initialize_metadata_pointer(token_program, mint, Some(*authority), Some(*mint))
            .expect("metadata pointer initialization failed"),
        initialize_default_account_state(token_program, mint, &AccountState::Frozen)
            .expect("default account state initialization failed"),
        initialize_mint_close_authority(token_program, mint, Some(authority))
            .expect("mint close authority initialization failed"),
    ]
}

pub fn read_transfer_fee_config(
    mint_data: &[u8],
) -> Result<
    spl_token_2022::extension::transfer_fee::TransferFeeConfig,
    solana_program_error::ProgramError,
> {
    use spl_token_2022::extension::{
        transfer_fee::TransferFeeConfig, BaseStateWithExtensions, StateWithExtensions,
    };

    let mint = StateWithExtensions::<Mint>::unpack(mint_data)?;
    Ok(*mint.get_extension::<TransferFeeConfig>()?)
}

pub fn read_transfer_fee_amount(
    account_data: &[u8],
) -> Result<u64, solana_program_error::ProgramError> {
    use spl_token_2022::extension::{
        transfer_fee::TransferFeeAmount, BaseStateWithExtensions, StateWithExtensions,
    };

    let account = StateWithExtensions::<spl_token_2022::state::Account>::unpack(account_data)?;
    let fee = account.get_extension::<TransferFeeAmount>()?;

    Ok(u64::from(fee.withheld_amount))
}

pub fn read_token_account(
    account_data: &[u8],
) -> Result<spl_token_2022::state::Account, solana_program_error::ProgramError> {
    use spl_token_2022::extension::StateWithExtensions;

    let account = StateWithExtensions::<spl_token_2022::state::Account>::unpack(account_data)?;
    Ok(account.base)
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

pub fn initialize_token_account(
    token_program: &Pubkey,
    account: &Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Instruction {
    spl_token_2022::instruction::initialize_account3(token_program, account, mint, owner)
        .expect("failed to build initialize_account3 instruction")
}

pub fn thaw_token_account(
    token_program: &Pubkey,
    account: &Pubkey,
    mint: &Pubkey,
    freeze_authority: &Pubkey,
) -> Instruction {
    spl_token_2022::instruction::thaw_account(token_program, account, mint, freeze_authority, &[])
        .expect("failed to build thaw_account instruction")
}

pub fn token_account_space() -> usize {
    let required_extensions = ExtensionType::get_required_init_account_extensions(&v1_extensions());

    ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(&required_extensions)
        .expect("failed to calculate token account size")
}

pub fn v2_token_account_space() -> usize {
    let mut account_extensions =
        ExtensionType::get_required_init_account_extensions(&v2_extensions());

    account_extensions.push(ExtensionType::ConfidentialTransferAccount);
    account_extensions.push(ExtensionType::ConfidentialTransferFeeAmount);

    ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(&account_extensions)
        .expect("failed to calculate V2 token account size")
}

pub fn mint_tokens(
    token_program: &Pubkey,
    mint: &Pubkey,
    account: &Pubkey,
    mint_authority: &Pubkey,
    amount: u64,
) -> Instruction {
    spl_token_2022::instruction::mint_to(token_program, mint, account, mint_authority, &[], amount)
        .expect("failed to build mint_to instruction")
}

pub fn v2_extensions() -> Vec<ExtensionType> {
    vec![
        ExtensionType::TransferFeeConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::DefaultAccountState,
        ExtensionType::MintCloseAuthority,
        ExtensionType::PermanentDelegate,
        ExtensionType::ConfidentialTransferMint,
        ExtensionType::ConfidentialTransferFeeConfig,
    ]
}

pub fn v2_mint_space() -> usize {
    ExtensionType::try_calculate_account_len::<Mint>(&v2_extensions())
        .expect("failed to calculate V2 mint size")
}

pub fn v2_initialization_instructions(
    token_program: &Pubkey,
    mint: &Pubkey,
    authority: &Pubkey,
    permanent_delegate: &Pubkey,
    confidential_fee_elgamal_pubkey: Option<&solana_zk_sdk::encryption::elgamal::ElGamalPubkey>,
) -> Vec<Instruction> {
    let mut instructions = v1_initialization_instructions(token_program, mint, authority);

    instructions.push(
        spl_token_2022::instruction::initialize_permanent_delegate(
            token_program,
            mint,
            permanent_delegate,
        )
        .expect("permanent delegate initialization failed"),
    );

    instructions.push(
        spl_token_2022::extension::confidential_transfer::instruction::initialize_mint(
            token_program,
            mint,
            Some(*authority),
            false,
            None,
        )
        .expect("confidential transfer mint initialization failed"),
    );

    instructions.push(
        spl_token_2022::extension::confidential_transfer_fee::instruction::initialize_confidential_transfer_fee_config(
            token_program,
            mint,
            Some(*authority),
            &(*confidential_fee_elgamal_pubkey.expect("confidential fee ElGamal pubkey required for V2")).into(),
        )
        .expect("confidential transfer fee initialization failed"),
    );

    instructions
}

pub fn initialize_v2_token_account(
    token_program: &Pubkey,
    account: &Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Instruction {
    initialize_token_account(token_program, account, mint, owner)
}

pub fn read_confidential_transfer_mint(
    mint_data: &[u8],
) -> Result<ConfidentialTransferMint, solana_program_error::ProgramError> {
    use spl_token_2022::extension::{BaseStateWithExtensions, StateWithExtensions};

    let mint = StateWithExtensions::<Mint>::unpack(mint_data)?;
    Ok(*mint.get_extension::<ConfidentialTransferMint>()?)
}

pub fn read_confidential_transfer_account(
    account_data: &[u8],
) -> Result<ConfidentialTransferAccount, solana_program_error::ProgramError> {
    use spl_token_2022::extension::{BaseStateWithExtensions, StateWithExtensions};

    let account = StateWithExtensions::<spl_token_2022::state::Account>::unpack(account_data)?;
    Ok(*account.get_extension::<ConfidentialTransferAccount>()?)
}

pub fn read_confidential_transfer_fee_amount(
    account_data: &[u8],
) -> Result<ConfidentialTransferFeeAmount, solana_program_error::ProgramError> {
    use spl_token_2022::extension::{BaseStateWithExtensions, StateWithExtensions};

    let account = StateWithExtensions::<spl_token_2022::state::Account>::unpack(account_data)?;
    Ok(*account.get_extension::<ConfidentialTransferFeeAmount>()?)
}
