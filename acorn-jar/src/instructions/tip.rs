use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{pubkey_eq, Pubkey},
    ProgramResult,
};

use crate::{
    error::JarError,
    state::{self, Jar},
    token,
    util::{require_signer, require_writable},
};

/// Maximum tip, in whole tokens.
pub const MAX_TIP_TOKENS: u64 = 1_000_000;

/// Largest allowed tip in base units for a mint with `decimals` decimals.
pub fn max_tip(decimals: u8) -> u64 {
    10u64
        .checked_pow(decimals as u32)
        .and_then(|unit| unit.checked_mul(MAX_TIP_TOKENS))
        .unwrap_or(u64::MAX)
}

/// Accounts: [sender (signer), sender token account (writable), vault (writable),
/// jar (writable), mint, token program]
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let [sender, sender_token, vault, jar, mint, token_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    require_signer(sender)?;
    require_writable(sender_token)?;
    require_writable(vault)?;
    require_writable(jar)?;

    if amount == 0 {
        return Err(JarError::ZeroTip.into());
    }

    let tip_count = {
        let data = jar.try_borrow_data()?;
        let state = Jar::load(program_id, jar, &data)?;
        if !pubkey_eq(token_program.key(), state.token_program()) {
            return Err(JarError::WrongTokenProgram.into());
        }
        if !pubkey_eq(mint.key(), state.mint()) {
            return Err(JarError::WrongMint.into());
        }
        if !pubkey_eq(vault.key(), state.vault()) {
            return Err(JarError::WrongVault.into());
        }
        state.tip_count()
    };

    let decimals = token::validate_mint(mint, token_program.key())?;
    if amount > max_tip(decimals) {
        return Err(JarError::TipTooLarge.into());
    }
    token::validate_token_account(sender_token, token_program.key(), mint.key())?;
    if pubkey_eq(sender_token.key(), vault.key()) {
        return Err(ProgramError::InvalidArgument);
    }

    let new_count = tip_count
        .checked_add(1)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    token::transfer_checked(
        token_program,
        sender_token,
        mint,
        vault,
        sender,
        amount,
        decimals,
        &[],
    )?;

    let mut data = jar.try_borrow_mut_data()?;
    state::set_tip_count(&mut data, new_count);
    Ok(())
}
