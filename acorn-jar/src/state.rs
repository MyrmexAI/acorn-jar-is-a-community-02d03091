use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

use crate::error::JarError;

pub const JAR_SEED: &[u8] = b"jar";
pub const VAULT_SEED: &[u8] = b"vault";

pub const JAR_DISCRIMINATOR: u8 = 1;

const OFF_BUMP: usize = 1;
const OFF_VAULT_BUMP: usize = 2;
const OFF_OWNER: usize = 3;
const OFF_MINT: usize = 35;
const OFF_VAULT: usize = 67;
const OFF_TOKEN_PROGRAM: usize = 99;
const OFF_TIP_COUNT: usize = 131;
pub const JAR_LEN: usize = 139;

/// Read-only typed view over the jar account data.
pub struct Jar<'a>(&'a [u8]);

impl<'a> Jar<'a> {
    /// Validates the account is a jar created by this program.
    pub fn load(program_id: &Pubkey, account: &AccountInfo, data: &'a [u8]) -> Result<Self, ProgramError> {
        if !account.is_owned_by(program_id) || data.len() != JAR_LEN || data[0] != JAR_DISCRIMINATOR {
            return Err(JarError::InvalidJar.into());
        }
        Ok(Self(data))
    }

    fn key_at(&self, off: usize) -> &Pubkey {
        self.0[off..off + 32].try_into().unwrap()
    }

    pub fn bump(&self) -> u8 {
        self.0[OFF_BUMP]
    }
    pub fn vault_bump(&self) -> u8 {
        self.0[OFF_VAULT_BUMP]
    }
    pub fn owner(&self) -> &Pubkey {
        self.key_at(OFF_OWNER)
    }
    pub fn mint(&self) -> &Pubkey {
        self.key_at(OFF_MINT)
    }
    pub fn vault(&self) -> &Pubkey {
        self.key_at(OFF_VAULT)
    }
    pub fn token_program(&self) -> &Pubkey {
        self.key_at(OFF_TOKEN_PROGRAM)
    }
    pub fn tip_count(&self) -> u64 {
        u64::from_le_bytes(self.0[OFF_TIP_COUNT..OFF_TIP_COUNT + 8].try_into().unwrap())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn init_jar(
    data: &mut [u8],
    bump: u8,
    vault_bump: u8,
    owner: &Pubkey,
    mint: &Pubkey,
    vault: &Pubkey,
    token_program: &Pubkey,
) {
    data[0] = JAR_DISCRIMINATOR;
    data[OFF_BUMP] = bump;
    data[OFF_VAULT_BUMP] = vault_bump;
    data[OFF_OWNER..OFF_OWNER + 32].copy_from_slice(owner);
    data[OFF_MINT..OFF_MINT + 32].copy_from_slice(mint);
    data[OFF_VAULT..OFF_VAULT + 32].copy_from_slice(vault);
    data[OFF_TOKEN_PROGRAM..OFF_TOKEN_PROGRAM + 32].copy_from_slice(token_program);
    data[OFF_TIP_COUNT..OFF_TIP_COUNT + 8].copy_from_slice(&0u64.to_le_bytes());
}

pub fn set_tip_count(data: &mut [u8], count: u64) {
    data[OFF_TIP_COUNT..OFF_TIP_COUNT + 8].copy_from_slice(&count.to_le_bytes());
}
