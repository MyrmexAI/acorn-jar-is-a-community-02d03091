use pinocchio::{
    account_info::AccountInfo,
    instruction::Signer,
    program_error::ProgramError,
    pubkey::{pubkey_eq, Pubkey},
    sysvars::{rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer};

pub fn require_signer(account: &AccountInfo) -> ProgramResult {
    if account.is_signer() {
        Ok(())
    } else {
        Err(ProgramError::MissingRequiredSignature)
    }
}

pub fn require_writable(account: &AccountInfo) -> ProgramResult {
    if account.is_writable() {
        Ok(())
    } else {
        Err(ProgramError::InvalidArgument)
    }
}

pub fn require_key(account: &AccountInfo, key: &Pubkey) -> ProgramResult {
    if pubkey_eq(account.key(), key) {
        Ok(())
    } else {
        Err(ProgramError::InvalidArgument)
    }
}

/// Creates a PDA-owned account. Works even if someone pre-funded the address with lamports.
/// The account must currently be system-owned and empty.
pub fn create_pda_account(
    payer: &AccountInfo,
    account: &AccountInfo,
    space: usize,
    owner: &Pubkey,
    signers: &[Signer],
) -> ProgramResult {
    if !account.is_owned_by(&pinocchio_system::ID) || !account.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    let required = Rent::get()?.minimum_balance(space);
    if account.lamports() == 0 {
        CreateAccount {
            from: payer,
            to: account,
            lamports: required,
            space: space as u64,
            owner,
        }
        .invoke_signed(signers)
    } else {
        let missing = required.saturating_sub(account.lamports());
        if missing > 0 {
            Transfer {
                from: payer,
                to: account,
                lamports: missing,
            }
            .invoke()?;
        }
        Allocate {
            account,
            space: space as u64,
        }
        .invoke_signed(signers)?;
        Assign { account, owner }.invoke_signed(signers)
    }
}
