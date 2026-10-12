use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum JarError {
    ZeroTip = 0,
    TipTooLarge = 1,
    NotOwner = 2,
    UnsupportedMint = 3,
    WrongMint = 4,
    WrongVault = 5,
    WrongTokenProgram = 6,
    EmptyVault = 7,
    InvalidJar = 8,
    InvalidDestination = 9,
}

impl From<JarError> for ProgramError {
    fn from(e: JarError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
