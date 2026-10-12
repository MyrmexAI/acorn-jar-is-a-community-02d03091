use pinocchio::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction, Signer},
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::{pubkey_eq, Pubkey},
    ProgramResult,
};

use crate::error::JarError;

pub const TOKEN_PROGRAM: Pubkey = pinocchio_pubkey::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022_PROGRAM: Pubkey =
    pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

pub const MINT_LEN: usize = 82;
pub const TOKEN_ACCOUNT_LEN: usize = 165;

const MINT_DECIMALS_OFF: usize = 44;
const MINT_INITIALIZED_OFF: usize = 45;
const ACCOUNT_TYPE_OFF: usize = 165;
const ACCOUNT_TYPE_MINT: u8 = 1;
const TLV_START: usize = 166;

const ACCOUNT_MINT_OFF: usize = 0;
const ACCOUNT_AMOUNT_OFF: usize = 64;
const ACCOUNT_STATE_OFF: usize = 108;
const ACCOUNT_STATE_INITIALIZED: u8 = 1;

/// Token-2022 mint extensions that cannot change who controls or how much arrives in
/// the vault: MetadataPointer, TokenMetadata, GroupPointer, TokenGroup,
/// GroupMemberPointer, TokenGroupMember. Everything else is refused. In particular
/// MintCloseAuthority (a closed mint address can be re-created with a PermanentDelegate),
/// PermanentDelegate, TransferHook, TransferFeeConfig, DefaultAccountState, Pausable and
/// the confidential-transfer extensions are rejected.
fn extension_allowed(ext_type: u16) -> bool {
    matches!(ext_type, 18..=23)
}

pub fn is_token_program(key: &Pubkey) -> bool {
    pubkey_eq(key, &TOKEN_PROGRAM) || pubkey_eq(key, &TOKEN_2022_PROGRAM)
}

/// Validates the mint and returns its decimals. The mint must be an initialized mint
/// owned by `token_program` (SPL Token or Token-2022) without any unsafe extension.
pub fn validate_mint(mint: &AccountInfo, token_program: &Pubkey) -> Result<u8, ProgramError> {
    if !is_token_program(token_program) || !mint.is_owned_by(token_program) {
        return Err(JarError::WrongTokenProgram.into());
    }
    let data = mint.try_borrow_data()?;
    if data.len() < MINT_LEN || data[MINT_INITIALIZED_OFF] != 1 {
        return Err(JarError::UnsupportedMint.into());
    }
    if data.len() > MINT_LEN {
        if !pubkey_eq(token_program, &TOKEN_2022_PROGRAM)
            || data.len() <= TLV_START
            || data[ACCOUNT_TYPE_OFF] != ACCOUNT_TYPE_MINT
        {
            return Err(JarError::UnsupportedMint.into());
        }
        let mut rest = &data[TLV_START..];
        while !rest.is_empty() {
            if rest.len() < 4 {
                return Err(JarError::UnsupportedMint.into());
            }
            let ext_type = u16::from_le_bytes([rest[0], rest[1]]);
            let len = u16::from_le_bytes([rest[2], rest[3]]) as usize;
            if !extension_allowed(ext_type) || rest.len() < 4 + len {
                return Err(JarError::UnsupportedMint.into());
            }
            rest = &rest[4 + len..];
        }
    }
    Ok(data[MINT_DECIMALS_OFF])
}

/// Checks `account` is an initialized token account of `token_program` holding `mint`.
pub fn validate_token_account(
    account: &AccountInfo,
    token_program: &Pubkey,
    mint: &Pubkey,
) -> Result<(), ProgramError> {
    if !account.is_owned_by(token_program) {
        return Err(ProgramError::IllegalOwner);
    }
    let data = account.try_borrow_data()?;
    if data.len() < TOKEN_ACCOUNT_LEN
        || data[ACCOUNT_STATE_OFF] != ACCOUNT_STATE_INITIALIZED
        || !pubkey_eq(
            data[ACCOUNT_MINT_OFF..ACCOUNT_MINT_OFF + 32].try_into().unwrap(),
            mint,
        )
    {
        return Err(JarError::WrongMint.into());
    }
    Ok(())
}

pub fn token_account_amount(account: &AccountInfo) -> Result<u64, ProgramError> {
    let data = account.try_borrow_data()?;
    if data.len() < TOKEN_ACCOUNT_LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(u64::from_le_bytes(
        data[ACCOUNT_AMOUNT_OFF..ACCOUNT_AMOUNT_OFF + 8].try_into().unwrap(),
    ))
}

/// InitializeAccount3: the account must already be allocated and owned by the token program.
pub fn initialize_account3(
    token_program: &AccountInfo,
    account: &AccountInfo,
    mint: &AccountInfo,
    owner: &Pubkey,
) -> ProgramResult {
    let mut data = [0u8; 33];
    data[0] = 18;
    data[1..].copy_from_slice(owner);
    let metas = [AccountMeta::writable(account.key()), AccountMeta::readonly(mint.key())];
    let ix = Instruction {
        program_id: token_program.key(),
        accounts: &metas,
        data: &data,
    };
    invoke_signed(&ix, &[account, mint], &[])
}

pub fn transfer_checked(
    token_program: &AccountInfo,
    from: &AccountInfo,
    mint: &AccountInfo,
    to: &AccountInfo,
    authority: &AccountInfo,
    amount: u64,
    decimals: u8,
    signers: &[Signer],
) -> ProgramResult {
    let mut data = [0u8; 10];
    data[0] = 12;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data[9] = decimals;
    let metas = [
        AccountMeta::writable(from.key()),
        AccountMeta::readonly(mint.key()),
        AccountMeta::writable(to.key()),
        AccountMeta::readonly_signer(authority.key()),
    ];
    let ix = Instruction {
        program_id: token_program.key(),
        accounts: &metas,
        data: &data,
    };
    invoke_signed(&ix, &[from, mint, to, authority], signers)
}
