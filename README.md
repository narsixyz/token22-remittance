# Token-2022 Remittance
A Rust implementation and test suite for a compliance-aware remittance stablecoin built with Solana Token-2022 extensions.
## Implementation
### V1 Mint
The initial mint contains:
- `TransferFeeConfig`
- `MetadataPointer`, pointing to the mint itself
- `DefaultAccountState`, initialized as `Frozen`
- `MintCloseAuthority`
Mint and token-account sizes are calculated with `ExtensionType::try_calculate_account_len`.
All extension initialization instructions are emitted before `InitializeMint`.
New token accounts therefore begin frozen and require the freeze authority to thaw them after the KYC step.
### Transfer Fees
Transfers use `TransferCheckedWithFee`.
The fee is calculated from the current epoch configuration:
```rust
transfer_fee.calculate_epoch_fee(current_epoch, amount)

Mint and token-account extension state is read through StateWithExtensions.

V2 Confidential Mint

Confidential transfers cannot be added to an already-created mint, so V2 re-issues the mint with:

* TransferFeeConfig
* MetadataPointer
* DefaultAccountState
* MintCloseAuthority
* PermanentDelegate
* ConfidentialTransferMint
* ConfidentialTransferFeeConfig

The confidential transfer fee configuration uses a dedicated ElGamal public key.

V2 token accounts include:

* ConfidentialTransferAccount
* ConfidentialTransferFeeAmount

Confidential Lifecycle

The integration test covers:

1. Token-account creation
2. Frozen-by-default accounts
3. KYC thaw
4. Token minting
5. Owner-only ConfigureAccount
6. Manual confidential-account approval
7. Confidential deposit
8. Pending-balance application
9. Confidential TransferWithFee
10. Recipient pending-balance application
11. Confidential withdrawal
12. Required zero-knowledge proof verification

ConfigureAccount is explicitly tested as an owner-only operation and is separate from token-account creation.

Permanent Delegate and Confidential Balances

The V2 mint has a permanent delegate for normal token balances.

Confidential balances are different: their amounts are encrypted and validated through zero-knowledge proofs. If a sanctioned user moves tokens into confidential state before the permanent delegate acts, the delegate cannot simply inspect the hidden amount and perform the same transparent seizure flow.

This is a compliance design gap that requires confidential-transfer-specific authority, recovery, approval, or enforcement mechanisms.

Token-2022 v9 Test Fixture

The tests use an exact Token-2022 v9 SBF fixture:

tests/fixtures/spl_token_2022.so

The fixture is required because the solana-program-test version used here embeds a different Token-2022 runtime by default. Confidential-transfer proof generation is version-sensitive, so the host-side proof generation and runtime verifier must use compatible Token-2022 versions.

The test harness therefore enables BPF execution and explicitly loads the repository fixture.

SHA-256:

672378b05ebd921eedb5159515a3c9d00ad1cc52f1fca80fd7a541de06a54ff1

Project Structure

.
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── lib.rs
│   └── token22.rs
└── tests/
    ├── assignment.rs
    └── fixtures/
        └── spl_token_2022.so

Tests

Run:

cargo test

Current result:

running 6 tests
test result: ok. 6 passed; 0 failed

The suite covers:

* Current-epoch transfer fee calculation
* TransferCheckedWithFee
* V1 mint extensions
* V2 mint extensions
* Account sizing
* Frozen-account KYC flow
* Confidential account configuration
* Manual approval
* Confidential deposits
* Pending-balance application
* Confidential TransferWithFee
* Confidential withdrawals
* Confidential transfer fees
* Zero-knowledge proof verification
    EOF


