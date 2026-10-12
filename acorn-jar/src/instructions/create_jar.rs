use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{find_program_address, pubkey_eq, Pubkey},
    ProgramResult,
};

use crate::{
    state::{self, JAR_LEN, JAR_SEED, VAULT_SEED},
    token::{self, TOKEN_ACCOUNT_LEN},
    util::{create_pda_account, require_key, require_signer, require_writable},
};

/// Accounts: [owner (signer, writable), jar (writable), vault (writable), mint,
/// system program, token program]
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let [owner, jar, vault, mint, system_program, token_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    require_signer(owner)?;
    require_writable(owner)?;
    require_writable(jar)?;
    require_writable(vault)?;
    require_key(system_program, &pinocchio_system::ID)?;

    token::validate_mint(mint, token_program.key())?;

    let (jar_key, jar_bump) =
        find_program_address(&[JAR_SEED, owner.key().as_ref(), mint.key().as_ref()], program_id);
    if !pubkey_eq(jar.key(), &jar_key) {
        return Err(ProgramError::InvalidSeeds);
    }
    let (vault_key, vault_bump) =
        find_program_address(&[VAULT_SEED, jar_key.as_ref()], program_id);
    if !pubkey_eq(vault.key(), &vault_key) {
        return Err(ProgramError::InvalidSeeds);
    }

    let jar_bump_seed = [jar_bump];
    let jar_seeds = [
        Seed::from(JAR_SEED),
        Seed::from(owner.key().as_ref()),
        Seed::from(mint.key().as_ref()),
        Seed::from(&jar_bump_seed),
    ];
    create_pda_account(owner, jar, JAR_LEN, program_id, &[Signer::from(&jar_seeds)])?;

    let vault_bump_seed = [vault_bump];
    let vault_seeds = [
        Seed::from(VAULT_SEED),
        Seed::from(jar_key.as_ref()),
        Seed::from(&vault_bump_seed),
    ];
    create_pda_account(
        owner,
        vault,
        TOKEN_ACCOUNT_LEN,
        token_program.key(),
        &[Signer::from(&vault_seeds)],
    )?;
    token::initialize_account3(token_program, vault, mint, &jar_key)?;

    let mut data = jar.try_borrow_mut_data()?;
    state::init_jar(
        &mut data,
        jar_bump,
        vault_bump,
        owner.key(),
        mint.key(),
        vault.key(),
        token_program.key(),
    );
    Ok(())
}
