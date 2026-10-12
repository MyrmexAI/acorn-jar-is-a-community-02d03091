use litesvm::{
    types::{FailedTransactionMetadata, TransactionMetadata},
    LiteSVM,
};
use solana_instruction::{error::InstructionError, AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;
use solana_transaction::Transaction;
use solana_transaction_error::TransactionError;

const TOKEN: Pubkey = Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const TOKEN_2022: Pubkey = Pubkey::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

// Custom error codes of the program (see src/error.rs).
const ZERO_TIP: u32 = 0;
const TIP_TOO_LARGE: u32 = 1;
const NOT_OWNER: u32 = 2;
const UNSUPPORTED_MINT: u32 = 3;
const WRONG_MINT: u32 = 4;
const WRONG_VAULT: u32 = 5;
const WRONG_TOKEN_PROGRAM: u32 = 6;
const EMPTY_VAULT: u32 = 7;

type Sent = Result<TransactionMetadata, FailedTransactionMetadata>;

enum Ext {
    None,
    MintCloseAuthority,
    PermanentDelegate,
    MetadataPointer,
}

struct Env {
    svm: LiteSVM,
    program: Pubkey,
    payer: Keypair,
}

fn program_so() -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let so = root.join("target/deploy/acorn_jar.so");
    if !so.exists() {
        let status = std::process::Command::new("cargo")
            .arg("build-sbf")
            .current_dir(root)
            .status()
            .expect("run cargo build-sbf");
        assert!(status.success(), "cargo build-sbf failed");
    }
    so
}

impl Env {
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let program = Pubkey::new_from_array(acorn_jar::ID);
        svm.add_program_from_file(program, program_so()).unwrap();
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
        Env { svm, program, payer }
    }

    fn funded(&mut self) -> Keypair {
        let kp = Keypair::new();
        self.svm.airdrop(&kp.pubkey(), 10_000_000_000).unwrap();
        kp
    }

    fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> Sent {
        self.svm.expire_blockhash();
        let tx = Transaction::new_signed_with_payer(
            ixs,
            Some(&signers[0].pubkey()),
            signers,
            self.svm.latest_blockhash(),
        );
        self.svm.send_transaction(tx)
    }

    fn create_mint(&mut self, token_program: Pubkey, decimals: u8, ext: Ext) -> Pubkey {
        let mint = Keypair::new();
        let authority = self.payer.pubkey();
        let (space, ext_ix): (u64, Option<Instruction>) = match ext {
            Ext::None => (82, None),
            Ext::MintCloseAuthority => {
                let mut data = vec![25, 1];
                data.extend_from_slice(authority.as_ref());
                (202, Some(Instruction::new_with_bytes(
                    token_program,
                    &data,
                    vec![AccountMeta::new(mint.pubkey(), false)],
                )))
            }
            Ext::PermanentDelegate => {
                let mut data = vec![35];
                data.extend_from_slice(authority.as_ref());
                (202, Some(Instruction::new_with_bytes(
                    token_program,
                    &data,
                    vec![AccountMeta::new(mint.pubkey(), false)],
                )))
            }
            Ext::MetadataPointer => {
                let mut data = vec![39, 0];
                data.extend_from_slice(authority.as_ref());
                data.extend_from_slice(mint.pubkey().as_ref());
                (234, Some(Instruction::new_with_bytes(
                    token_program,
                    &data,
                    vec![AccountMeta::new(mint.pubkey(), false)],
                )))
            }
        };
        let lamports = self.svm.minimum_balance_for_rent_exemption(space as usize);
        let mut ixs = vec![system_instruction::create_account(
            &authority,
            &mint.pubkey(),
            lamports,
            space,
            &token_program,
        )];
        ixs.extend(ext_ix);
        let mut init = vec![20, decimals];
        init.extend_from_slice(authority.as_ref());
        init.push(0);
        ixs.push(Instruction::new_with_bytes(
            token_program,
            &init,
            vec![AccountMeta::new(mint.pubkey(), false)],
        ));
        let payer = self.payer.insecure_clone();
        self.send(&ixs, &[&payer, &mint]).expect("create mint");
        mint.pubkey()
    }

    fn create_token_account(&mut self, token_program: Pubkey, mint: &Pubkey, owner: &Pubkey) -> Pubkey {
        let acct = Keypair::new();
        let lamports = self.svm.minimum_balance_for_rent_exemption(165);
        let mut init = vec![18];
        init.extend_from_slice(owner.as_ref());
        let ixs = [
            system_instruction::create_account(&self.payer.pubkey(), &acct.pubkey(), lamports, 165, &token_program),
            Instruction::new_with_bytes(
                token_program,
                &init,
                vec![AccountMeta::new(acct.pubkey(), false), AccountMeta::new_readonly(*mint, false)],
            ),
        ];
        let payer = self.payer.insecure_clone();
        self.send(&ixs, &[&payer, &acct]).expect("create token account");
        acct.pubkey()
    }

    fn mint_to(&mut self, token_program: Pubkey, mint: &Pubkey, dest: &Pubkey, amount: u64) {
        let mut data = vec![7];
        data.extend_from_slice(&amount.to_le_bytes());
        let ix = Instruction::new_with_bytes(
            token_program,
            &data,
            vec![
                AccountMeta::new(*mint, false),
                AccountMeta::new(*dest, false),
                AccountMeta::new_readonly(self.payer.pubkey(), true),
            ],
        );
        let payer = self.payer.insecure_clone();
        self.send(&[ix], &[&payer]).expect("mint_to");
    }

    fn balance(&self, token_account: &Pubkey) -> u64 {
        let acct = self.svm.get_account(token_account).expect("token account");
        u64::from_le_bytes(acct.data[64..72].try_into().unwrap())
    }

    fn jar_pda(&self, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[b"jar", owner.as_ref(), mint.as_ref()], &self.program).0
    }

    fn vault_pda(&self, jar: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[b"vault", jar.as_ref()], &self.program).0
    }

    fn create_jar_ix(&self, owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Instruction {
        let jar = self.jar_pda(owner, mint);
        let vault = self.vault_pda(&jar);
        Instruction::new_with_bytes(
            self.program,
            &[0],
            vec![
                AccountMeta::new(*owner, true),
                AccountMeta::new(jar, false),
                AccountMeta::new(vault, false),
                AccountMeta::new_readonly(*mint, false),
                AccountMeta::new_readonly(solana_system_interface::program::ID, false),
                AccountMeta::new_readonly(*token_program, false),
            ],
        )
    }

    fn tip_ix(
        &self,
        sender: &Pubkey,
        sender_token: &Pubkey,
        jar_owner: &Pubkey,
        mint: &Pubkey,
        token_program: &Pubkey,
        amount: u64,
    ) -> Instruction {
        let jar = self.jar_pda(jar_owner, mint);
        let vault = self.vault_pda(&jar);
        let mut data = vec![1];
        data.extend_from_slice(&amount.to_le_bytes());
        Instruction::new_with_bytes(
            self.program,
            &data,
            vec![
                AccountMeta::new_readonly(*sender, true),
                AccountMeta::new(*sender_token, false),
                AccountMeta::new(vault, false),
                AccountMeta::new(jar, false),
                AccountMeta::new_readonly(*mint, false),
                AccountMeta::new_readonly(*token_program, false),
            ],
        )
    }

    fn withdraw_ix(
        &self,
        signer: &Pubkey,
        jar_owner: &Pubkey,
        destination: &Pubkey,
        mint: &Pubkey,
        token_program: &Pubkey,
    ) -> Instruction {
        let jar = self.jar_pda(jar_owner, mint);
        let vault = self.vault_pda(&jar);
        Instruction::new_with_bytes(
            self.program,
            &[2],
            vec![
                AccountMeta::new_readonly(*signer, true),
                AccountMeta::new_readonly(jar, false),
                AccountMeta::new(vault, false),
                AccountMeta::new(*destination, false),
                AccountMeta::new_readonly(*mint, false),
                AccountMeta::new_readonly(*token_program, false),
            ],
        )
    }

    fn tip_count(&self, owner: &Pubkey, mint: &Pubkey) -> u64 {
        let jar = self.svm.get_account(&self.jar_pda(owner, mint)).expect("jar");
        u64::from_le_bytes(jar.data[131..139].try_into().unwrap())
    }
}

fn custom(result: Sent) -> u32 {
    match result.expect_err("transaction should fail").err {
        TransactionError::InstructionError(_, InstructionError::Custom(code)) => code,
        other => panic!("expected custom program error, got {other:?}"),
    }
}

fn failed_with(result: Sent, expected: InstructionError) {
    match result.expect_err("transaction should fail").err {
        TransactionError::InstructionError(_, err) => assert_eq!(err, expected),
        other => panic!("expected instruction error, got {other:?}"),
    }
}

/// A funded owner with a jar for a fresh mint, and a tipper holding tokens.
struct Scenario {
    env: Env,
    token_program: Pubkey,
    mint: Pubkey,
    owner: Keypair,
    owner_token: Pubkey,
    tipper: Keypair,
    tipper_token: Pubkey,
}

fn scenario(token_program: Pubkey, decimals: u8) -> Scenario {
    let mut env = Env::new();
    let mint = env.create_mint(token_program, decimals, Ext::None);
    let owner = env.funded();
    let tipper = env.funded();
    let owner_token = env.create_token_account(token_program, &mint, &owner.pubkey());
    let tipper_token = env.create_token_account(token_program, &mint, &tipper.pubkey());
    env.mint_to(token_program, &mint, &tipper_token, u64::MAX / 2);
    let ix = env.create_jar_ix(&owner.pubkey(), &mint, &token_program);
    env.send(&[ix], &[&owner]).expect("create jar");
    Scenario { env, token_program, mint, owner, owner_token, tipper, tipper_token }
}

impl Scenario {
    fn tip(&mut self, amount: u64) -> Sent {
        let ix = self.env.tip_ix(
            &self.tipper.pubkey(),
            &self.tipper_token,
            &self.owner.pubkey(),
            &self.mint,
            &self.token_program,
            amount,
        );
        let tipper = self.tipper.insecure_clone();
        self.env.send(&[ix], &[&tipper])
    }

    fn vault(&self) -> Pubkey {
        let jar = self.env.jar_pda(&self.owner.pubkey(), &self.mint);
        self.env.vault_pda(&jar)
    }
}

// ---------- create ----------

fn check_create(token_program: Pubkey) {
    let s = scenario(token_program, 6);
    let jar_key = s.env.jar_pda(&s.owner.pubkey(), &s.mint);
    let jar = s.env.svm.get_account(&jar_key).expect("jar exists");
    assert_eq!(jar.owner, s.env.program);
    assert_eq!(jar.data.len(), 139);
    assert_eq!(&jar.data[3..35], s.owner.pubkey().as_ref());
    assert_eq!(&jar.data[35..67], s.mint.as_ref());
    assert_eq!(&jar.data[67..99], s.vault().as_ref());
    assert_eq!(&jar.data[99..131], token_program.as_ref());
    assert_eq!(s.env.tip_count(&s.owner.pubkey(), &s.mint), 0);

    let vault = s.env.svm.get_account(&s.vault()).expect("vault exists");
    assert_eq!(vault.owner, token_program);
    assert_eq!(&vault.data[0..32], s.mint.as_ref(), "vault holds the jar mint");
    assert_eq!(&vault.data[32..64], jar_key.as_ref(), "vault is owned by the jar PDA");
    assert_eq!(s.env.balance(&s.vault()), 0);
}

#[test]
fn create_jar_spl_token() {
    check_create(TOKEN);
}

#[test]
fn create_jar_token_2022() {
    check_create(TOKEN_2022);
}

#[test]
fn create_jar_twice_fails() {
    let mut s = scenario(TOKEN, 6);
    let ix = s.env.create_jar_ix(&s.owner.pubkey(), &s.mint, &TOKEN);
    let owner = s.owner.insecure_clone();
    assert!(s.env.send(&[ix], &[&owner]).is_err());
}

#[test]
fn create_jar_requires_owner_signature() {
    let mut env = Env::new();
    let mint = env.create_mint(TOKEN, 6, Ext::None);
    let owner = Keypair::new();
    let mut ix = env.create_jar_ix(&owner.pubkey(), &mint, &TOKEN);
    ix.accounts[0].is_signer = false;
    let payer = env.payer.insecure_clone();
    failed_with(env.send(&[ix], &[&payer]), InstructionError::MissingRequiredSignature);
}

#[test]
fn create_jar_with_prefunded_vault_address_succeeds() {
    let mut env = Env::new();
    let mint = env.create_mint(TOKEN, 6, Ext::None);
    let owner = env.funded();
    let jar = env.jar_pda(&owner.pubkey(), &mint);
    let vault = env.vault_pda(&jar);
    let payer = env.payer.insecure_clone();
    let grief = system_instruction::transfer(&payer.pubkey(), &vault, 1);
    env.send(&[grief], &[&payer]).unwrap();
    let ix = env.create_jar_ix(&owner.pubkey(), &mint, &TOKEN);
    env.send(&[ix], &[&owner]).expect("create jar despite prefunded vault");
    assert_eq!(env.svm.get_account(&vault).unwrap().owner, TOKEN);
}

#[test]
fn create_jar_with_wrong_token_program_fails() {
    let mut env = Env::new();
    let mint = env.create_mint(TOKEN, 6, Ext::None);
    let owner = env.funded();
    let ix = env.create_jar_ix(&owner.pubkey(), &mint, &TOKEN_2022);
    assert_eq!(custom(env.send(&[ix], &[&owner])), WRONG_TOKEN_PROGRAM);
}

#[test]
fn create_jar_with_non_mint_fails() {
    let mut env = Env::new();
    let owner = env.funded();
    let fake_mint = env.create_mint(TOKEN, 6, Ext::None);
    let token_account = env.create_token_account(TOKEN, &fake_mint, &owner.pubkey());
    let ix = env.create_jar_ix(&owner.pubkey(), &token_account, &TOKEN);
    assert_eq!(custom(env.send(&[ix], &[&owner])), UNSUPPORTED_MINT);
}

// ---------- unsafe Token-2022 mint extensions ----------

#[test]
fn closeable_token_2022_mint_is_rejected() {
    // A closeable mint could be closed and re-created at the same address with a
    // PermanentDelegate, which could then drain the vault.
    let mut env = Env::new();
    let mint = env.create_mint(TOKEN_2022, 6, Ext::MintCloseAuthority);
    let owner = env.funded();
    let ix = env.create_jar_ix(&owner.pubkey(), &mint, &TOKEN_2022);
    assert_eq!(custom(env.send(&[ix], &[&owner])), UNSUPPORTED_MINT);
    assert!(env.svm.get_account(&env.jar_pda(&owner.pubkey(), &mint)).is_none());
}

#[test]
fn permanent_delegate_token_2022_mint_is_rejected() {
    let mut env = Env::new();
    let mint = env.create_mint(TOKEN_2022, 6, Ext::PermanentDelegate);
    let owner = env.funded();
    let ix = env.create_jar_ix(&owner.pubkey(), &mint, &TOKEN_2022);
    assert_eq!(custom(env.send(&[ix], &[&owner])), UNSUPPORTED_MINT);
}

#[test]
fn metadata_pointer_token_2022_mint_is_accepted() {
    let mut env = Env::new();
    let mint = env.create_mint(TOKEN_2022, 6, Ext::MetadataPointer);
    let owner = env.funded();
    let ix = env.create_jar_ix(&owner.pubkey(), &mint, &TOKEN_2022);
    env.send(&[ix], &[&owner]).expect("create jar");
}

// ---------- tip ----------

fn check_tip(token_program: Pubkey) {
    let mut s = scenario(token_program, 6);
    let before = s.env.balance(&s.tipper_token);
    s.tip(5_000_000).expect("tip");
    assert_eq!(s.env.balance(&s.vault()), 5_000_000);
    assert_eq!(s.env.balance(&s.tipper_token), before - 5_000_000);
    assert_eq!(s.env.tip_count(&s.owner.pubkey(), &s.mint), 1);

    s.tip(1).expect("tip 1 base unit");
    s.tip(250).expect("tip again");
    assert_eq!(s.env.balance(&s.vault()), 5_000_251);
    assert_eq!(s.env.tip_count(&s.owner.pubkey(), &s.mint), 3);
}

#[test]
fn tip_spl_token() {
    check_tip(TOKEN);
}

#[test]
fn tip_token_2022() {
    check_tip(TOKEN_2022);
}

#[test]
fn tip_zero_fails() {
    let mut s = scenario(TOKEN, 6);
    assert_eq!(custom(s.tip(0)), ZERO_TIP);
    assert_eq!(s.env.balance(&s.vault()), 0);
    assert_eq!(s.env.tip_count(&s.owner.pubkey(), &s.mint), 0);
}

#[test]
fn tip_above_max_fails() {
    let mut s = scenario(TOKEN, 6);
    let max = 1_000_000 * 1_000_000u64;
    assert_eq!(custom(s.tip(max + 1)), TIP_TOO_LARGE);
    assert_eq!(custom(s.tip(u64::MAX)), TIP_TOO_LARGE);
    assert_eq!(s.env.balance(&s.vault()), 0);
    assert_eq!(s.env.tip_count(&s.owner.pubkey(), &s.mint), 0);
}

#[test]
fn tip_exactly_max_succeeds() {
    let mut s = scenario(TOKEN, 6);
    let max = 1_000_000 * 1_000_000u64;
    s.tip(max).expect("max tip");
    assert_eq!(s.env.balance(&s.vault()), max);
}

#[test]
fn max_tip_scales_with_decimals() {
    let mut s = scenario(TOKEN_2022, 9);
    let max = 1_000_000 * 1_000_000_000u64;
    assert_eq!(custom(s.tip(max + 1)), TIP_TOO_LARGE);
    s.tip(max).expect("max tip with 9 decimals");

    let mut s0 = scenario(TOKEN, 0);
    assert_eq!(custom(s0.tip(1_000_001)), TIP_TOO_LARGE);
    s0.tip(1_000_000).expect("max tip with 0 decimals");
}

#[test]
fn tip_requires_sender_signature() {
    let mut s = scenario(TOKEN, 6);
    let mut ix = s.env.tip_ix(
        &s.tipper.pubkey(),
        &s.tipper_token,
        &s.owner.pubkey(),
        &s.mint,
        &TOKEN,
        10,
    );
    ix.accounts[0].is_signer = false;
    let owner = s.owner.insecure_clone();
    failed_with(s.env.send(&[ix], &[&owner]), InstructionError::MissingRequiredSignature);
}

#[test]
fn tip_cannot_spend_someone_elses_tokens() {
    let mut s = scenario(TOKEN, 6);
    // The owner signs but points at the tipper's token account.
    let ix = s.env.tip_ix(
        &s.owner.pubkey(),
        &s.tipper_token,
        &s.owner.pubkey(),
        &s.mint,
        &TOKEN,
        10,
    );
    let owner = s.owner.insecure_clone();
    assert!(s.env.send(&[ix], &[&owner]).is_err());
    assert_eq!(s.env.balance(&s.vault()), 0);
}

#[test]
fn tip_of_another_mint_is_rejected() {
    let mut s = scenario(TOKEN, 6);
    let other_mint = s.env.create_mint(TOKEN, 6, Ext::None);
    let other_token = s.env.create_token_account(TOKEN, &other_mint, &s.tipper.pubkey());
    s.env.mint_to(TOKEN, &other_mint, &other_token, 1_000);

    // Other mint's tokens into this jar's vault.
    let mut ix = s.env.tip_ix(&s.tipper.pubkey(), &other_token, &s.owner.pubkey(), &s.mint, &TOKEN, 10);
    let tipper = s.tipper.insecure_clone();
    assert_eq!(custom(s.env.send(&[ix.clone()], &[&tipper])), WRONG_MINT);

    // Claim the other mint to get past the mint check: jar mint differs.
    ix.accounts[4].pubkey = other_mint;
    assert_eq!(custom(s.env.send(&[ix], &[&tipper])), WRONG_MINT);

    assert_eq!(s.env.balance(&s.vault()), 0);
    assert_eq!(s.env.balance(&other_token), 1_000);
}

#[test]
fn tip_with_foreign_vault_is_rejected() {
    let mut s = scenario(TOKEN, 6);
    let stranger_account = s.env.create_token_account(TOKEN, &s.mint, &s.owner.pubkey());
    let mut ix = s.env.tip_ix(&s.tipper.pubkey(), &s.tipper_token, &s.owner.pubkey(), &s.mint, &TOKEN, 10);
    ix.accounts[2].pubkey = stranger_account;
    let tipper = s.tipper.insecure_clone();
    assert_eq!(custom(s.env.send(&[ix], &[&tipper])), WRONG_VAULT);
}

#[test]
fn tip_with_wrong_token_program_is_rejected() {
    let mut s = scenario(TOKEN, 6);
    let mut ix = s.env.tip_ix(&s.tipper.pubkey(), &s.tipper_token, &s.owner.pubkey(), &s.mint, &TOKEN, 10);
    ix.accounts[5].pubkey = TOKEN_2022;
    let tipper = s.tipper.insecure_clone();
    assert_eq!(custom(s.env.send(&[ix], &[&tipper])), WRONG_TOKEN_PROGRAM);
}

#[test]
fn tip_to_nonexistent_jar_fails() {
    let mut s = scenario(TOKEN, 6);
    let nobody = Keypair::new();
    let ix = s.env.tip_ix(&s.tipper.pubkey(), &s.tipper_token, &nobody.pubkey(), &s.mint, &TOKEN, 10);
    let tipper = s.tipper.insecure_clone();
    assert!(s.env.send(&[ix], &[&tipper]).is_err());
}

// ---------- withdraw ----------

fn check_withdraw(token_program: Pubkey) {
    let mut s = scenario(token_program, 6);
    s.tip(7_000_000).unwrap();
    s.tip(3_000_000).unwrap();
    assert_eq!(s.env.balance(&s.vault()), 10_000_000);

    let ix = s.env.withdraw_ix(&s.owner.pubkey(), &s.owner.pubkey(), &s.owner_token, &s.mint, &token_program);
    let owner = s.owner.insecure_clone();
    s.env.send(&[ix], &[&owner]).expect("owner withdraws");
    assert_eq!(s.env.balance(&s.vault()), 0);
    assert_eq!(s.env.balance(&s.owner_token), 10_000_000);
    assert_eq!(s.env.tip_count(&s.owner.pubkey(), &s.mint), 2);

    // The jar keeps working after a withdrawal.
    s.tip(42).unwrap();
    assert_eq!(s.env.balance(&s.vault()), 42);
}

#[test]
fn withdraw_by_owner_spl_token() {
    check_withdraw(TOKEN);
}

#[test]
fn withdraw_by_owner_token_2022() {
    check_withdraw(TOKEN_2022);
}

#[test]
fn withdraw_by_stranger_fails() {
    let mut s = scenario(TOKEN, 6);
    s.tip(1_000_000).unwrap();

    let stranger = s.env.funded();
    let stranger_token = s.env.create_token_account(TOKEN, &s.mint, &stranger.pubkey());

    // The stranger signs for themselves, naming the real jar owner's jar.
    let ix = s.env.withdraw_ix(&stranger.pubkey(), &s.owner.pubkey(), &stranger_token, &s.mint, &TOKEN);
    assert_eq!(custom(s.env.send(&[ix], &[&stranger])), NOT_OWNER);

    // The stranger names the real owner but cannot sign for them.
    let mut ix = s.env.withdraw_ix(&s.owner.pubkey(), &s.owner.pubkey(), &stranger_token, &s.mint, &TOKEN);
    ix.accounts[0].is_signer = false;
    failed_with(s.env.send(&[ix], &[&stranger]), InstructionError::MissingRequiredSignature);

    // The stranger tries their own jar's PDA derivation against the real vault.
    let own_jar_ix = s.env.create_jar_ix(&stranger.pubkey(), &s.mint, &TOKEN);
    s.env.send(&[own_jar_ix], &[&stranger]).unwrap();
    let mut ix = s.env.withdraw_ix(&stranger.pubkey(), &stranger.pubkey(), &stranger_token, &s.mint, &TOKEN);
    ix.accounts[2].pubkey = s.vault();
    assert_eq!(custom(s.env.send(&[ix], &[&stranger])), WRONG_VAULT);

    assert_eq!(s.env.balance(&s.vault()), 1_000_000);
    assert_eq!(s.env.balance(&stranger_token), 0);
}

#[test]
fn withdraw_empty_vault_fails() {
    let mut s = scenario(TOKEN, 6);
    let ix = s.env.withdraw_ix(&s.owner.pubkey(), &s.owner.pubkey(), &s.owner_token, &s.mint, &TOKEN);
    let owner = s.owner.insecure_clone();
    assert_eq!(custom(s.env.send(&[ix], &[&owner])), EMPTY_VAULT);
}

#[test]
fn withdraw_to_account_of_another_mint_fails() {
    let mut s = scenario(TOKEN, 6);
    s.tip(1_000).unwrap();
    let other_mint = s.env.create_mint(TOKEN, 6, Ext::None);
    let other_token = s.env.create_token_account(TOKEN, &other_mint, &s.owner.pubkey());
    let ix = s.env.withdraw_ix(&s.owner.pubkey(), &s.owner.pubkey(), &other_token, &s.mint, &TOKEN);
    let owner = s.owner.insecure_clone();
    assert_eq!(custom(s.env.send(&[ix], &[&owner])), WRONG_MINT);
    assert_eq!(s.env.balance(&s.vault()), 1_000);
}

// ---------- instruction decoding ----------

#[test]
fn unknown_instruction_and_bad_data_fail() {
    let mut s = scenario(TOKEN, 6);
    let owner = s.owner.insecure_clone();
    for data in [vec![], vec![9], vec![1, 1, 2], vec![0, 0], vec![2, 0]] {
        let ix = Instruction::new_with_bytes(
            s.env.program,
            &data,
            vec![AccountMeta::new(owner.pubkey(), true)],
        );
        assert!(s.env.send(&[ix], &[&owner]).is_err(), "data {data:?} must fail");
    }
}
