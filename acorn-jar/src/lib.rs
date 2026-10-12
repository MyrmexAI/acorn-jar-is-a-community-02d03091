#![no_std]

use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey,
    ProgramResult,
};

pub mod error;
pub mod instructions;
pub mod state;
pub mod token;
pub mod util;

pinocchio_pubkey::declare_id!("4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T");

pinocchio::program_entrypoint!(process_instruction, 6);
pinocchio::default_allocator!();
pinocchio::nostd_panic_handler!();

pub const CREATE_JAR: u8 = 0;
pub const TIP: u8 = 1;
pub const WITHDRAW: u8 = 2;

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let (tag, rest) = data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    match *tag {
        CREATE_JAR => {
            if !rest.is_empty() {
                return Err(ProgramError::InvalidInstructionData);
            }
            instructions::create_jar::process(program_id, accounts)
        }
        TIP => {
            let amount: [u8; 8] = rest
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?;
            instructions::tip::process(program_id, accounts, u64::from_le_bytes(amount))
        }
        WITHDRAW => {
            if !rest.is_empty() {
                return Err(ProgramError::InvalidInstructionData);
            }
            instructions::withdraw::process(program_id, accounts)
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
