use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{pubkey_eq, Pubkey},
    ProgramResult,
};

use crate::{
    error::JarError,
    state::{Jar, JAR_SEED},
    token,
    util::{require_signer, require_writable},
};

/// Accounts: [owner (signer), jar, vault (writable), destination token account (writable),
/// mint, token program]
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let [owner, jar, vault, destination, mint, token_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    require_signer(owner)?;
    require_writable(vault)?;
    require_writable(destination)?;

    let data = jar.try_borrow_data()?;
    let state = Jar::load(program_id, jar, &data)?;
    if !pubkey_eq(owner.key(), state.owner()) {
        return Err(JarError::NotOwner.into());
    }
    if !pubkey_eq(token_program.key(), state.token_program()) {
        return Err(JarError::WrongTokenProgram.into());
    }
    if !pubkey_eq(mint.key(), state.mint()) {
        return Err(JarError::WrongMint.into());
    }
    if !pubkey_eq(vault.key(), state.vault()) {
        return Err(JarError::WrongVault.into());
    }
    if pubkey_eq(destination.key(), vault.key()) {
        return Err(JarError::InvalidDestination.into());
    }
    token::validate_token_account(destination, token_program.key(), mint.key())?;
    let decimals = token::validate_mint(mint, token_program.key())?;

    let amount = token::token_account_amount(vault)?;
    if amount == 0 {
        return Err(JarError::EmptyVault.into());
    }

    let bump = [state.bump()];
    let seeds = [
        Seed::from(JAR_SEED),
        Seed::from(state.owner().as_ref()),
        Seed::from(state.mint().as_ref()),
        Seed::from(&bump),
    ];
    token::transfer_checked(
        token_program,
        vault,
        mint,
        destination,
        jar,
        amount,
        decimals,
        &[Signer::from(&seeds)],
    )
}
