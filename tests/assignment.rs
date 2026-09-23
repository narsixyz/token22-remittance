use solana_clock::Clock;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_program_test::{BanksClient, BanksClientError, ProgramTest};
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
use token22_remittance::token22::{
    read_confidential_transfer_account, read_confidential_transfer_fee_amount,
    read_confidential_transfer_mint, v2_initialization_instructions, v2_mint_space,
    v2_token_account_space,
};
use token22_remittance::token22::{v1_initialization_instructions, v1_mint_space};

async fn process_instructions(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    instructions: &[Instruction],
    signers: &[&Keypair],
) -> Result<(), BanksClientError> {
    let recent_blockhash = banks_client.get_latest_blockhash().await?;
    let mut transaction_signers = Vec::with_capacity(signers.len() + 1);
    transaction_signers.push(payer);
    transaction_signers.extend_from_slice(signers);

    let transaction = solana_transaction::Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        transaction_signers.as_slice(),
        recent_blockhash,
    );

    banks_client.process_transaction(transaction).await
}

#[tokio::test]
async fn create_v1_mint_fixture() {
    let test = ProgramTest::default();
    let (banks_client, payer, recent_blockhash) = test.start().await;
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
    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    let token_program = spl_token_2022::id();
    let mint = Keypair::new();

    let mint_authority = Keypair::new();
    let freeze_authority = Keypair::new();
    let permanent_delegate = Keypair::new();
    let fee_withdraw_elgamal = solana_zk_sdk::encryption::elgamal::ElGamalKeypair::new_rand();

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
        Some(fee_withdraw_elgamal.pubkey()),
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

    let mint_data = banks_client
        .get_account(mint.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;
    let confidential_mint = read_confidential_transfer_mint(&mint_data).unwrap();

    assert!(!bool::from(&confidential_mint.auto_approve_new_accounts));
}

#[test]
fn task5_v2_account_space_includes_confidential_fee_extensions() {
    use spl_token_2022::{extension::ExtensionType, state::Account};

    let expected_extensions = vec![
        ExtensionType::TransferFeeAmount,
        ExtensionType::ConfidentialTransferAccount,
        ExtensionType::ConfidentialTransferFeeAmount,
    ];
    let expected_space = ExtensionType::try_calculate_account_len::<Account>(&expected_extensions)
        .expect("failed to calculate expected V2 account size");

    assert_eq!(v2_token_account_space(), expected_space);
}

#[tokio::test]
async fn task6_confidential_lifecycle_with_manual_approval_and_fees() {
    use solana_zk_sdk::{
        encryption::{
            auth_encryption::AeKey,
            elgamal::{ElGamalCiphertext, ElGamalKeypair},
        },
        zk_elgamal_proof_program::proof_data::{
            PubkeyValidityProofData,
            ZkProofData,
        },
    };
    use spl_token_2022::extension::confidential_transfer::instruction as confidential_instruction;
    use spl_token_confidential_transfer_proof_extraction::instruction::ProofLocation;
    use spl_token_confidential_transfer_proof_extraction::transfer_with_fee::TransferWithFeeProofContext;
    use spl_token_confidential_transfer_proof_generation::{
        transfer_with_fee::transfer_with_fee_split_proof_data, withdraw::withdraw_proof_data,
    };
    use std::num::NonZeroI8;

    const DECIMALS: u8 = 6;
    const DEPOSIT_AMOUNT: u64 = 2_000_000;
    const TRANSFER_AMOUNT: u64 = 500_000;
    const TRANSFER_FEE: u64 = 5_000;
    const WITHDRAW_AMOUNT: u64 = 200_000;

    let mut program_test = ProgramTest::default();
    program_test.prefer_bpf(true);
    program_test.add_program("spl_token_2022", spl_token_2022::id(), None);
    let (mut banks_client, payer, _recent_blockhash) = program_test.start().await;

    let token_program = spl_token_2022::id();
    let mint = Keypair::new();
    let mint_authority = Keypair::new();
    let freeze_authority = Keypair::new();
    let permanent_delegate = Keypair::new();
    let alice_account = Keypair::new();
    let bob_account = Keypair::new();
    let alice_owner = Keypair::new();
    let bob_owner = Keypair::new();
    let attacker = Keypair::new();

    let alice_elgamal = ElGamalKeypair::new_rand();
    let bob_elgamal = ElGamalKeypair::new_rand();
    let fee_withdraw_elgamal = ElGamalKeypair::new_rand();
    let alice_ae_key = AeKey::new_rand();
    let bob_ae_key = AeKey::new_rand();

    let mint_space = v2_mint_space();
    let account_space = v2_token_account_space();
    let rent = banks_client.get_rent().await.expect("failed to get rent");
    let mint_rent = rent.minimum_balance(mint_space);
    let account_rent = rent.minimum_balance(account_space);

    let mut setup_instructions = vec![
        system_instruction::create_account(
            &payer.pubkey(),
            &mint.pubkey(),
            mint_rent,
            mint_space as u64,
            &token_program,
        ),
        system_instruction::create_account(
            &payer.pubkey(),
            &alice_account.pubkey(),
            account_rent,
            account_space as u64,
            &token_program,
        ),
        system_instruction::create_account(
            &payer.pubkey(),
            &bob_account.pubkey(),
            account_rent,
            account_space as u64,
            &token_program,
        ),
    ];
    setup_instructions.extend(v2_initialization_instructions(
        &token_program,
        &mint.pubkey(),
        &mint_authority.pubkey(),
        &permanent_delegate.pubkey(),
        Some(fee_withdraw_elgamal.pubkey()),
    ));
    setup_instructions.push(
        initialize_mint(
            &token_program,
            &mint.pubkey(),
            &mint_authority.pubkey(),
            Some(&freeze_authority.pubkey()),
            DECIMALS,
        )
        .expect("failed to build initialize mint"),
    );
    setup_instructions.push(token22_remittance::token22::initialize_v2_token_account(
        &token_program,
        &alice_account.pubkey(),
        &mint.pubkey(),
        &alice_owner.pubkey(),
    ));
    setup_instructions.push(token22_remittance::token22::initialize_v2_token_account(
        &token_program,
        &bob_account.pubkey(),
        &mint.pubkey(),
        &bob_owner.pubkey(),
    ));
    setup_instructions.push(thaw_token_account(
        &token_program,
        &alice_account.pubkey(),
        &mint.pubkey(),
        &freeze_authority.pubkey(),
    ));
    setup_instructions.push(thaw_token_account(
        &token_program,
        &bob_account.pubkey(),
        &mint.pubkey(),
        &freeze_authority.pubkey(),
    ));
    setup_instructions.push(mint_tokens(
        &token_program,
        &mint.pubkey(),
        &alice_account.pubkey(),
        &mint_authority.pubkey(),
        DEPOSIT_AMOUNT,
    ));

    process_instructions(
        &mut banks_client,
        &payer,
        &setup_instructions,
        &[
            &mint,
            &alice_account,
            &bob_account,
            &freeze_authority,
            &mint_authority,
        ],
    )
    .await
    .expect("failed to initialize V2 fixture");

    let alice_zero_balance = alice_ae_key.encrypt(0).into();
    let alice_pubkey_proof = PubkeyValidityProofData::new(&alice_elgamal).unwrap();
    let attacker_configure = confidential_instruction::configure_account(
        &token_program,
        &alice_account.pubkey(),
        &mint.pubkey(),
        &alice_zero_balance,
        16,
        &attacker.pubkey(),
        &[],
        ProofLocation::InstructionOffset(NonZeroI8::new(1).unwrap(), &alice_pubkey_proof),
    )
    .expect("failed to build attacker configure account");

    assert!(
        process_instructions(&mut banks_client, &payer, &attacker_configure, &[&attacker],)
            .await
            .is_err(),
        "ConfigureAccount must be signed by the token-account owner"
    );

    let alice_configure = confidential_instruction::configure_account(
        &token_program,
        &alice_account.pubkey(),
        &mint.pubkey(),
        &alice_zero_balance,
        16,
        &alice_owner.pubkey(),
        &[],
        ProofLocation::InstructionOffset(NonZeroI8::new(1).unwrap(), &alice_pubkey_proof),
    )
    .expect("failed to build Alice configure account");

    process_instructions(&mut banks_client, &payer, &alice_configure, &[&alice_owner])
        .await
        .expect("Alice confidential configure failed");

    let alice_data = banks_client
        .get_account(alice_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;
    let alice_confidential = read_confidential_transfer_account(&alice_data).unwrap();
    assert!(!bool::from(&alice_confidential.approved));
    assert_eq!(
        read_confidential_transfer_fee_amount(&alice_data)
            .unwrap()
            .withheld_amount,
        Default::default()
    );

    let bob_zero_balance = bob_ae_key.encrypt(0).into();
    let bob_pubkey_proof = PubkeyValidityProofData::new(&bob_elgamal).unwrap();
    let bob_configure = confidential_instruction::configure_account(
        &token_program,
        &bob_account.pubkey(),
        &mint.pubkey(),
        &bob_zero_balance,
        16,
        &bob_owner.pubkey(),
        &[],
        ProofLocation::InstructionOffset(NonZeroI8::new(1).unwrap(), &bob_pubkey_proof),
    )
    .expect("failed to build Bob configure account");

    process_instructions(&mut banks_client, &payer, &bob_configure, &[&bob_owner])
        .await
        .expect("Bob confidential configure failed");

    let approve_alice = confidential_instruction::approve_account(
        &token_program,
        &alice_account.pubkey(),
        &mint.pubkey(),
        &mint_authority.pubkey(),
        &[],
    )
    .expect("failed to build Alice approval");
    let approve_bob = confidential_instruction::approve_account(
        &token_program,
        &bob_account.pubkey(),
        &mint.pubkey(),
        &mint_authority.pubkey(),
        &[],
    )
    .expect("failed to build Bob approval");

    process_instructions(
        &mut banks_client,
        &payer,
        &[approve_alice, approve_bob],
        &[&mint_authority],
    )
    .await
    .expect("manual confidential account approval failed");

    let alice_data = banks_client
        .get_account(alice_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;
    assert!(bool::from(
        &read_confidential_transfer_account(&alice_data)
            .unwrap()
            .approved
    ));

    let deposit = confidential_instruction::deposit(
        &token_program,
        &alice_account.pubkey(),
        &mint.pubkey(),
        DEPOSIT_AMOUNT,
        DECIMALS,
        &alice_owner.pubkey(),
        &[],
    )
    .expect("failed to build deposit");

    process_instructions(&mut banks_client, &payer, &[deposit], &[&alice_owner])
        .await
        .expect("confidential deposit failed");

    let alice_data = banks_client
        .get_account(alice_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;
    let alice_base = read_token_account(&alice_data).unwrap();
    let alice_confidential = read_confidential_transfer_account(&alice_data).unwrap();
    assert_eq!(alice_base.amount, 0);
    assert_eq!(
        u64::from(alice_confidential.pending_balance_credit_counter),
        1
    );
    assert_eq!(alice_confidential.available_balance, Default::default());

    let alice_decryptable_after_deposit = alice_ae_key.encrypt(DEPOSIT_AMOUNT);
    let apply_alice_deposit = confidential_instruction::apply_pending_balance(
        &token_program,
        &alice_account.pubkey(),
        1,
        &alice_decryptable_after_deposit.into(),
        &alice_owner.pubkey(),
        &[],
    )
    .expect("failed to build apply pending balance");

    process_instructions(
        &mut banks_client,
        &payer,
        &[apply_alice_deposit],
        &[&alice_owner],
    )
    .await
    .expect("Alice apply pending balance failed");

    let alice_data = banks_client
        .get_account(alice_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;
    let alice_confidential = read_confidential_transfer_account(&alice_data).unwrap();
    assert_eq!(
        u64::from(alice_confidential.pending_balance_credit_counter),
        0
    );
    assert_ne!(alice_confidential.available_balance, Default::default());

    let current_available_balance =
        ElGamalCiphertext::try_from(alice_confidential.available_balance).unwrap();
    let mint_transfer_fee_config = token22_remittance::token22::read_transfer_fee_config(
        &banks_client
            .get_account(mint.pubkey())
            .await
            .expect("mint account missing")
            .expect("mint account missing")
            .data,
    )
    .expect("failed to read transfer fee config");

    let current_epoch = banks_client
        .get_sysvar::<Clock>()
        .await
        .expect("failed to read Clock")
        .epoch;

    let expected_fee = mint_transfer_fee_config
        .calculate_epoch_fee(current_epoch, TRANSFER_AMOUNT)
        .expect("failed to calculate runtime fee");
    assert_eq!(expected_fee, TRANSFER_FEE);

    let transfer_proof = transfer_with_fee_split_proof_data(
        &current_available_balance,
        &alice_decryptable_after_deposit,
        TRANSFER_AMOUNT,
        &alice_elgamal,
        &alice_ae_key,
        bob_elgamal.pubkey(),
        None,
        fee_withdraw_elgamal.pubkey(),
        100,
        1_000_000,
    )
    .expect("failed to generate transfer-with-fee proof");
    let alice_decryptable_after_transfer = alice_ae_key.encrypt(DEPOSIT_AMOUNT - TRANSFER_AMOUNT);
    TransferWithFeeProofContext::verify_and_extract(
        transfer_proof.equality_proof_data.context_data(),
        transfer_proof
            .transfer_amount_ciphertext_validity_proof_data_with_ciphertext
            .proof_data
            .context_data(),
        transfer_proof.percentage_with_cap_proof_data.context_data(),
        transfer_proof.fee_ciphertext_validity_proof_data.context_data(),
        transfer_proof.range_proof_data.context_data(),
        100,
        1_000_000,
    )
    .expect("transfer-with-fee proof contexts are inconsistent");
    let transfer = confidential_instruction::transfer_with_fee(
        &token_program,
        &alice_account.pubkey(),
        &mint.pubkey(),
        &bob_account.pubkey(),
        &alice_decryptable_after_transfer.into(),
        &transfer_proof
            .transfer_amount_ciphertext_validity_proof_data_with_ciphertext
            .ciphertext_lo,
        &transfer_proof
            .transfer_amount_ciphertext_validity_proof_data_with_ciphertext
            .ciphertext_hi,
        &alice_owner.pubkey(),
        &[],
        ProofLocation::InstructionOffset(
            NonZeroI8::new(1).unwrap(),
            &transfer_proof.equality_proof_data,
        ),
        ProofLocation::InstructionOffset(
            NonZeroI8::new(2).unwrap(),
            &transfer_proof
                .transfer_amount_ciphertext_validity_proof_data_with_ciphertext
                .proof_data,
        ),
        ProofLocation::InstructionOffset(
            NonZeroI8::new(3).unwrap(),
            &transfer_proof.percentage_with_cap_proof_data,
        ),
        ProofLocation::InstructionOffset(
            NonZeroI8::new(4).unwrap(),
            &transfer_proof.fee_ciphertext_validity_proof_data,
        ),
        ProofLocation::InstructionOffset(
            NonZeroI8::new(5).unwrap(),
            &transfer_proof.range_proof_data,
        ),
    )
    .expect("failed to build confidential transfer with fee");

    process_instructions(&mut banks_client, &payer, &transfer, &[&alice_owner])
        .await
        .expect("confidential transfer with fee failed");

    let bob_data = banks_client
        .get_account(bob_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;
    let bob_confidential = read_confidential_transfer_account(&bob_data).unwrap();
    assert_eq!(
        u64::from(bob_confidential.pending_balance_credit_counter),
        1
    );
    assert_eq!(bob_confidential.available_balance, Default::default());
    assert_ne!(
        read_confidential_transfer_fee_amount(&bob_data)
            .unwrap()
            .withheld_amount,
        Default::default()
    );

    let bob_net_received = TRANSFER_AMOUNT - TRANSFER_FEE;
    let bob_decryptable_after_transfer = bob_ae_key.encrypt(bob_net_received);
    let apply_bob_transfer = confidential_instruction::apply_pending_balance(
        &token_program,
        &bob_account.pubkey(),
        1,
        &bob_decryptable_after_transfer.into(),
        &bob_owner.pubkey(),
        &[],
    )
    .expect("failed to build Bob apply pending balance");

    process_instructions(
        &mut banks_client,
        &payer,
        &[apply_bob_transfer],
        &[&bob_owner],
    )
    .await
    .expect("Bob apply pending balance failed");

    let bob_data = banks_client
        .get_account(bob_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;
    let bob_confidential = read_confidential_transfer_account(&bob_data).unwrap();
    assert_ne!(bob_confidential.available_balance, Default::default());

    let bob_available_balance =
        ElGamalCiphertext::try_from(bob_confidential.available_balance).unwrap();
    let withdraw_proof = withdraw_proof_data(
        &bob_available_balance,
        bob_net_received,
        WITHDRAW_AMOUNT,
        &bob_elgamal,
    )
    .expect("failed to generate withdraw proof");
    let bob_decryptable_after_withdraw = bob_ae_key.encrypt(bob_net_received - WITHDRAW_AMOUNT);
    let withdraw = confidential_instruction::withdraw(
        &token_program,
        &bob_account.pubkey(),
        &mint.pubkey(),
        WITHDRAW_AMOUNT,
        DECIMALS,
        &bob_decryptable_after_withdraw.into(),
        &bob_owner.pubkey(),
        &[],
        ProofLocation::InstructionOffset(
            NonZeroI8::new(1).unwrap(),
            &withdraw_proof.equality_proof_data,
        ),
        ProofLocation::InstructionOffset(
            NonZeroI8::new(2).unwrap(),
            &withdraw_proof.range_proof_data,
        ),
    )
    .expect("failed to build withdraw");

    process_instructions(&mut banks_client, &payer, &withdraw, &[&bob_owner])
        .await
        .expect("confidential withdraw failed");

    let bob_data = banks_client
        .get_account(bob_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .data;
    let bob_base = read_token_account(&bob_data).unwrap();
    assert_eq!(bob_base.amount, WITHDRAW_AMOUNT);
}
